#!/usr/bin/env python3
"""Compare real Chrome/native grayscale text at intermediate foreground/background colors."""
import argparse
import html
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess

import cairo

ROOT = Path(__file__).resolve().parent.parent


def red(path):
    surface = cairo.ImageSurface.create_from_png(str(path))
    data = bytes(surface.get_data())
    return data[2::4]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'reports/text-colors')
    parser.add_argument('--chrome', default=shutil.which('google-chrome'))
    parser.add_argument('--background-control', action='store_true',
                        help='Hold foreground fixed while varying background')
    args = parser.parse_args()
    assert args.chrome, 'Chrome required'
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    page = runpy.run_path(str(ROOT / 'scripts/compare-chrome-text.py'))['page']
    results = []
    for kind in ('latin', 'cjk'):
        source = ROOT / 'reports/hint-targets-exact' / kind / '1' / 'case.json'
        original = json.loads(source.read_text())
        postscript = subprocess.check_output(['fc-match', '-f', '%{postscriptname}',
                                             original['fontFamily']], text=True).strip()
        pairs = ((160, 0), (160, 32), (160, 240), (160, 255)) if args.background_control else (
            (255, 0), (0, 255), (160, 32), (96, 240), (224, 64), (32, 192))
        for fg, bg in pairs:
            folder = out / f'{kind}-{fg}-{bg}'
            folder.mkdir(exist_ok=True)
            case = {**original, 'foreground': f'#{fg:02x}{fg:02x}{fg:02x}',
                    'background': f'#{bg:02x}{bg:02x}{bg:02x}'}
            manifest = folder / 'case.json'
            manifest.write_text(json.dumps(case, ensure_ascii=False))
            document = folder / 'case.html'
            document.write_text(page(case, 1, postscript, True))
            screenshot = folder / 'chrome.png'
            cmd = [args.chrome, '--headless=new', '--disable-background-networking',
                   '--no-first-run', '--no-default-browser-check', '--hide-scrollbars',
                   '--virtual-time-budget=2000', '--force-device-scale-factor=1',
                   f'--window-size={case["width"]},{case["height"]}',
                   f'--user-data-dir={folder / "profile"}', f'--screenshot={screenshot}',
                   '--dump-dom', document.as_uri()]
            if os.geteuid() == 0:
                cmd.insert(1, '--no-sandbox')
            chrome = subprocess.run(cmd, capture_output=True, text=True, check=True, timeout=45)
            assert 'data-state="ready"' in chrome.stdout
            metrics = json.loads(html.unescape(re.search(r'data-metrics="([^"]+)"', chrome.stdout)[1]))
            assert metrics['dpr'] == 1
            reference = red(screenshot)
            assert len(reference) == case['width'] * case['height']
            sign = 1 if fg > bg else -1
            reference_ink = sum(sign * (value - bg) for value in reference)
            assert reference_ink > 0
            for boost in (0, 2):
                native = folder / f'native-{boost}'
                subprocess.run(['cargo', 'run', '--locked', '--offline', '-p', 'plico-shell',
                                '--example', 'text_quality_probe', '--', '--cases', str(manifest),
                                '--output', str(native), '--exact-baselines'], cwd=ROOT,
                               env={**os.environ, 'PLICO_TEXT_STEM_BOOST': str(boost)},
                               check=True, stdout=subprocess.DEVNULL)
                candidate = red(native / 'plico-text-quality-1x.png')
                assert len(candidate) == len(reference)
                record = dict(font=kind, foreground=fg, background=bg, stem_boost=boost,
                              ink_ratio=sum(sign * (v-bg) for v in candidate) / reference_ink,
                              l1_percent=sum(abs(a-b) for a,b in zip(reference,candidate)) /
                              reference_ink * 100)
                results.append(record)
                print(record, flush=True)
    (out / 'measurements.json').write_text(json.dumps(dict(
        chrome=subprocess.check_output([args.chrome, '--version'], text=True).strip(),
        scale=1, results=results), indent=2))


if __name__ == '__main__':
    main()
