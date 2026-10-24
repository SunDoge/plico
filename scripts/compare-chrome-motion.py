#!/usr/bin/env python3
"""Compare Chrome text positioning and compositor transforms on the motion fixtures."""
import argparse
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
page = runpy.run_path(str(ROOT / 'scripts/compare-chrome-text.py'))['page']
samples = runpy.run_path(str(ROOT / 'scripts/compare-text-motion.py'))['samples']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'reports/chrome-text-motion')
    parser.add_argument('--chrome', default=shutil.which('google-chrome'))
    args = parser.parse_args()
    assert args.chrome
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    results = []
    for family in ('Noto Sans', 'Noto Sans CJK SC'):
        slug = family.replace(' ', '-')
        postscript = subprocess.check_output(['fc-match','-f','%{postscriptname}',family], text=True).strip()
        for axis in ('x', 'y'):
            case = json.loads((ROOT / 'reports/text-motion' / slug / axis / 'case.json').read_text())
            for mode in ('layout', 'transform', 'layer'):
                folder = out / slug / axis / mode
                folder.mkdir(parents=True, exist_ok=True)
                document = page(case, 1, postscript, True)
                if mode != 'layout':
                    # Restore integer layout origin, then apply the original
                    # fractional motion in a transform. Layer mode requests promotion.
                    document = document.replace('baselineErrors.push(', '''
                     const delta=index/32;
                     if (AXIS === 'x') div.style.left=(row.x-delta)+'px';
                     else div.style.top=(row.baseline-baseline-delta)+'px';
                     div.style.transform=AXIS === 'x' ? `translateX(${delta}px)` : `translateY(${delta}px)`;
                     if (LAYER) div.style.willChange='transform';
                     baselineErrors.push(''').replace('AXIS',json.dumps(axis)).replace('LAYER',str(mode=='layer').lower())
                path = folder / 'case.html'
                path.write_text(document)
                screenshot = folder / 'chrome.png'
                cmd = [args.chrome,'--headless=new','--disable-background-networking',
                    '--no-first-run','--no-default-browser-check','--hide-scrollbars',
                    '--virtual-time-budget=2000','--force-device-scale-factor=1',
                    f'--window-size={case["width"]},{case["height"]}',
                    f'--user-data-dir={folder / "profile"}',f'--screenshot={screenshot}',
                    '--dump-dom',path.as_uri()]
                if os.geteuid() == 0:
                    cmd.insert(1,'--no-sandbox')
                proc = subprocess.run(cmd,capture_output=True,text=True,check=True,timeout=45)
                assert 'data-state="ready"' in proc.stdout
                (folder / 'chrome.log').write_text(proc.stderr)
                record = dict(family=family,axis=axis,mode=mode,**samples(screenshot,axis))
                results.append(record)
                print({k:v for k,v in record.items() if k!='centroid_steps'},flush=True)
    (out / 'measurements.json').write_text(json.dumps(dict(
        chrome=subprocess.check_output([args.chrome,'--version'],text=True).strip(),
        requested_step=1/32, results=results),indent=2))


if __name__ == '__main__':
    main()
