#!/usr/bin/env python3
"""Validate the black/white-fitted Skia coverage model on held-out gray colors."""
import argparse
import json
from pathlib import Path
import runpy

ROOT = Path(__file__).resolve().parent.parent
red = runpy.run_path(str(ROOT / 'scripts/compare-text-colors.py'))['red']


def transfer(foreground, gamma, contrast):
    # Same source-luminance-dependent LUT construction as SkMaskGamma.cpp.
    src = foreground / 255
    dst = 1 - src  # Skia guesses the background; this is NOT the actual background.
    lin_src, lin_dst = src ** gamma, dst ** gamma
    result = []
    for index in range(256):
        a = index / 255
        a += (1-a) * contrast * lin_dst * a
        if abs(src-dst) >= 1/256:
            output = (lin_src*a + (1-a)*lin_dst) ** (1/gamma)
            a = (output-dst)/(src-dst)
        result.append(max(0, min(255, round(a*255))))
    return bytes(result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=ROOT / 'reports/text-colors')
    parser.add_argument('--model', type=Path, default=ROOT / 'reports/skia-coverage')
    args = parser.parse_args()
    model = json.loads((args.model / 'measurements.json').read_text())
    measurements = json.loads((args.source / 'measurements.json').read_text())
    results = []
    for record in measurements['results']:
        if record['stem_boost'] != 0:
            continue
        font, fg, bg = record['font'], record['foreground'], record['background']
        mask = red(args.model / f'{font}-1-full-quarter.png')
        reference = red(args.source / f'{font}-{fg}-{bg}' / 'chrome.png')
        assert len(mask) == len(reference)
        table = transfer(fg, model['gamma'], model['contrast'])
        mass = sum(abs(value-bg) for value in reference)
        def error(values):
            rendered = bytes(round(bg + (fg-bg)*a/255) for a in values)
            return sum(abs(a-b) for a,b in zip(rendered, reference))/mass*100
        result = dict(font=font, foreground=fg, background=bg,
                      before_percent=error(mask), after_percent=error(mask.translate(table)))
        results.append(result)
        print(result)
    (args.source / 'coverage-model.json').write_text(json.dumps(dict(
        training=model['training'], gamma=model['gamma'], contrast=model['contrast'],
        results=results), indent=2))
    # A background-only control must reconstruct the same effective coverage,
    # within 8-bit output rounding. Check browser and actual native renders.
    if (args.source / 'latin-160-0').exists():
        controls = []
        for font in ('latin', 'cjk'):
            for renderer in ('chrome.png', 'native-0/plico-text-quality-1x.png',
                             'native-2/plico-text-quality-1x.png'):
                base = red(args.source / f'{font}-160-0' / renderer)
                for bg in (32, 240, 255):
                    actual = red(args.source / f'{font}-160-{bg}' / renderer)
                    assert len(base) == len(actual)
                    maximum = max(abs(round(bg+(160-bg)*value/160)-pixel)
                                  for value,pixel in zip(base,actual))
                    assert maximum <= 1, (font, renderer, bg, maximum)
                    controls.append(dict(font=font, renderer=renderer, background=bg,
                                         max_channel_difference=maximum))
        (args.source / 'background-invariance.json').write_text(json.dumps(controls, indent=2))


if __name__ == '__main__':
    main()
