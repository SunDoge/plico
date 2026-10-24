#!/usr/bin/env python3
"""Isolate native quarter-pixel X quantization and hint policy against Chrome."""
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess

ROOT = Path(__file__).resolve().parent.parent
helpers = runpy.run_path(str(ROOT / 'scripts/analyze-text-colors.py'))
red, transfer = helpers['red'], helpers['transfer']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'reports/native-phase-control')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    model = json.loads((ROOT / 'reports/skia-coverage/measurements.json').read_text())
    results = []
    subprocess.run(['cargo', 'build', '--locked', '--offline', '-p', 'plico-shell',
                    '--example', 'text_quality_probe'], cwd=ROOT, check=True)
    for font in ('latin', 'cjk'):
        for scale in ('1', '1.25', '1.5', '2'):
            source = ROOT / 'reports/hint-targets-exact' / font / scale
            baseline_metrics = (source / 'vello_hint' / f'plico-text-quality-{scale}x.txt').read_text()
            for mode, flags in (('lcd-quarter', []), ('symmetric-quarter', ['--outline-mode', 'normal-ft'])):
                target = out / font / scale / mode
                subprocess.run(['cargo', 'run', '--locked', '--offline', '-p', 'plico-shell',
                                '--example', 'text_quality_probe', '--', '--cases', str(source / 'case.json'),
                                '--output', str(target), '--exact-baselines', '--x-steps', '4', *flags],
                               cwd=ROOT, env={**os.environ, 'PLICO_TEXT_STEM_BOOST': '0'},
                               check=True, stdout=subprocess.DEVNULL)
                def layout_lines(report):
                    return [line for line in report.splitlines()
                            if not line.strip().startswith('resolved_family=')]
                assert layout_lines((target / f'plico-text-quality-{scale}x.txt').read_text()) == layout_lines(baseline_metrics), 'layout must not change'
                mask = red(target / f'plico-text-quality-{scale}x.png')
                for foreground, suffix in ((255, ''), (0, '-light')):
                    reference = red(ROOT / f'reports/chrome-text-exact{suffix}' / font / scale / 'chrome.png')
                    if suffix:
                        reference = bytes(255-v for v in reference)
                    assert len(mask) == len(reference)
                    mass = sum(reference)
                    adjusted = mask.translate(transfer(foreground, model['gamma'], model['contrast']))
                    def error(candidate):
                        return sum(abs(a-b) for a,b in zip(reference,candidate))/mass*100
                    record = dict(font=font, scale=scale, mode=mode, foreground=foreground,
                                  raw_percent=error(mask), coverage_adjusted_percent=error(adjusted))
                    results.append(record)
                    print(record, flush=True)
    (out / 'measurements.json').write_text(json.dumps(dict(
        gamma=model['gamma'], contrast=model['contrast'], results=results), indent=2))


if __name__ == '__main__':
    main()
