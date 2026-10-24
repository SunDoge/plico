#!/usr/bin/env python3
"""Test a held-out coverage policy directly on native output, without replacing rasterization."""
import argparse
from collections import Counter
import json
from pathlib import Path
import runpy

ROOT = Path(__file__).resolve().parent.parent
colors = runpy.run_path(str(ROOT / 'scripts/analyze-text-colors.py'))
red, transfer = colors['red'], colors['transfer']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'reports/native-coverage-analysis')
    parser.add_argument('--source', type=Path, default=ROOT / 'reports/hint-targets-exact')
    parser.add_argument('--current', type=Path, default=ROOT / 'reports/current-text')
    parser.add_argument('--chrome', type=Path, default=ROOT / 'reports/chrome-text-exact')
    parser.add_argument('--scales', nargs='+', default=['1','1.25','1.5','2'])
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    model = json.loads((ROOT / 'reports/skia-coverage/measurements.json').read_text())
    results = []
    for font in ('latin', 'cjk'):
        for scale in args.scales:
            for boost, directory in (
                (0, args.source / font / scale / 'vello_hint'),
                ('compiled-default', args.current / font / scale),
            ):
                mask = red(directory / f'plico-text-quality-{scale}x.png')
                for foreground, suffix, invert in ((255, '', False), (0, '-light', True)):
                    reference = red(args.chrome.with_name(args.chrome.name+suffix) / font / scale / 'chrome.png')
                    if invert:
                        reference = bytes(255-v for v in reference)
                    assert len(mask) == len(reference)
                    mass = sum(reference)
                    adjusted = mask.translate(transfer(foreground, model['gamma'], model['contrast']))
                    def histogram(values):
                        counts = Counter(values)
                        return {f'{lo}-{hi}': sum(counts[value] for value in range(lo,hi+1))
                                for lo,hi in ((1,15),(16,63),(64,191),(192,229),(230,255))}
                    def error(candidate):
                        return sum(abs(a-b) for a,b in zip(reference,candidate))/mass*100
                    results.append(dict(font=font, scale=scale, boost=boost, foreground=foreground,
                                        before_percent=error(mask), after_percent=error(adjusted),
                                        ink_ratio_before=sum(mask)/mass, ink_ratio_after=sum(adjusted)/mass,
                                        reference_histogram=histogram(reference),
                                        native_histogram=histogram(mask),
                                        adjusted_histogram=histogram(adjusted)))
                    if scale == '1' and boost == 0:
                        # An intentionally overfit, unconstrained LUT gives a lower
                        # bound on residual error of any pointwise coverage policy.
                        histogram = Counter(zip(mask, reference))
                        lut = []
                        for value in range(256):
                            samples = sorted((target,count) for (source,target),count
                                             in histogram.items() if source == value)
                            total = sum(count for _,count in samples)
                            cumulative, median = 0, value
                            for target,count in samples:
                                cumulative += count
                                if cumulative * 2 >= total:
                                    median = target
                                    break
                            lut.append(median)
                        residual = sum(abs(lut[source]-target)*count
                                       for (source,target),count in histogram.items())/mass*100
                        results[-1]['oracle_lut_residual_percent'] = residual
    (args.output / 'measurements.json').write_text(json.dumps(dict(
        training=model['training'], gamma=model['gamma'], contrast=model['contrast'],
        results=results), indent=2))
    for record in results:
        print(record)


if __name__ == '__main__':
    main()
