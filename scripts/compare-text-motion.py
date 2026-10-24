#!/usr/bin/env python3
"""Sample single-glyph raster response to 1/32 physical-pixel motion (not frame pacing)."""
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess

import cairo

ROOT = Path(__file__).resolve().parent.parent
font_file = runpy.run_path(str(ROOT / 'scripts/compare-text-phases.py'))['font']


def samples(path, axis):
    image = cairo.ImageSurface.create_from_png(str(path))
    data = bytes(image.get_data())
    result = []
    for index in range(33):
        pixels, mass, moment = [], 0, 0
        for y in range(48):
            start = (index*48+y)*image.get_stride()
            line = data[start+2:start+image.get_width()*4:4]
            pixels.extend(line)
            for x,value in enumerate(line):
                mass += value
                moment += value * (x if axis == 'x' else y)
        assert mass > 0
        result.append(dict(pixels=bytes(pixels), centroid=moment/mass, mass=mass))
    steps = [b['centroid']-a['centroid'] for a,b in zip(result,result[1:])]
    return dict(identical_adjacent_steps=sum(a['pixels']==b['pixels'] for a,b in zip(result,result[1:])),
                centroid_steps=steps, min_centroid_step=min(steps), max_centroid_step=max(steps),
                total_centroid_motion=result[-1]['centroid']-result[0]['centroid'],
                ink_mass_range_percent=(max(r['mass'] for r in result)-min(r['mass'] for r in result)) /
                (sum(r['mass'] for r in result)/len(result))*100)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'reports/text-motion')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    results = []
    for family, glyph in (('Noto Sans', 'H'), ('Noto Sans CJK SC', '中')):
        file, _ = font_file(family)
        for axis in ('x', 'y'):
            folder = out / family.replace(' ', '-') / axis
            folder.mkdir(parents=True, exist_ok=True)
            rows = [dict(text=glyph, size=14, weight=400,
                         x=20+(i/32 if axis=='x' else 0),
                         baseline=32+i*48+(i/32 if axis=='y' else 0)) for i in range(33)]
            case = folder / 'case.json'
            case.write_text(json.dumps(dict(width=64, height=33*48, scales=[1],
                background='#000000', foreground='#ffffff', fontFamily=family,
                font_files=[file], rows=rows), ensure_ascii=False))
            for mode, flags in (('hint-continuous', []), ('hint-quarter', ['--x-steps', '4']),
                                ('nohint-continuous', ['--no-hint'])):
                target = folder / mode
                subprocess.run(['cargo','run','--locked','--offline','-p','plico-shell',
                    '--example','text_quality_probe','--','--cases',str(case),'--output',str(target),
                    '--exact-baselines',*flags], cwd=ROOT, check=True,
                    env={**os.environ,'PLICO_TEXT_STEM_BOOST':'0'}, stdout=subprocess.DEVNULL)
                record = dict(family=family, axis=axis, mode=mode,
                              **samples(target / 'plico-text-quality-1x.png', axis))
                results.append(record)
                print({k:v for k,v in record.items() if k!='centroid_steps'}, flush=True)
    (out / 'measurements.json').write_text(json.dumps(dict(
        requested_step=1/32, physical_scale=1, results=results), indent=2))


if __name__ == '__main__':
    main()
