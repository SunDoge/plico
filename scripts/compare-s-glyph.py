#!/usr/bin/env python3
"""Isolate lowercase s: native hinting, outline targets, snapping and local MyGO.

Requires Pycairo and an installed Noto Sans. Uses a read-only Go overlay.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import cairo


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=Path('reports/s-glyph'))
    parser.add_argument('--mygo', type=Path, default=Path('/home/me/code/go/mygo'))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    out = (root / args.output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    rows = [dict(text='s', size=size, weight=weight, x=20+p*40+p/8,
                 baseline=32+i*40)
            for i, (weight, size) in enumerate((w, s) for w in (400, 500) for s in (12, 14, 16))
            for p in range(8)]
    case = dict(width=350, height=260, background='#ffffff', foreground='#18181b',
                fontFamily='Noto Sans', scales=[1, 1.75], rows=rows)
    manifest = out / 'case.json'
    manifest.write_text(json.dumps(case, indent=2)+'\n')
    subprocess.run(['cargo', 'build', '--locked', '--offline', '-p', 'plico-shell',
                    '--example', 'text_quality_probe'], cwd=root, check=True)
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--no-deps', '--format-version=1', '--offline'], cwd=root))
    binary = Path(metadata['target_directory']) / 'debug/examples/text_quality_probe'
    env = os.environ.copy()
    env.pop('PLICO_TEXT_STEM_BOOST', None)
    variants = [('default', []), ('no-hint', ['--no-hint']),
                ('light', ['--outline-mode', 'light']), ('normal', ['--outline-mode', 'normal']),
                ('integer-x', ['--snap', 'xy'])]
    for name, flags in variants:
        with (out / f'{name}.log').open('w') as log:
            subprocess.run([str(binary), '--cases', str(manifest), '--output', str(out/name), *flags],
                           cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    overlay = out / 'overlay.json'
    overlay.write_text(json.dumps({'Replace': {
        str(args.mygo.resolve() / 'internal/raster/plico_comparison_test.go'):
        str(root / 'scripts/mygo-text-probe.go.txt')}}))
    ge = dict(env, CGO_ENABLED='0', PLICO_MYGO_CASE=str(manifest),
              PLICO_MYGO_OUTPUT=str(out), PLICO_MYGO_HINT='slight')
    with (out / 'mygo.log').open('w') as log:
        subprocess.run(['go', 'test', '-overlay', str(overlay), './internal/raster',
                        '-run', '^TestPlicoTextComparison$', '-count=1', '-v'],
                       cwd=args.mygo, env=ge, stdout=log, stderr=subprocess.STDOUT, check=True)
    stats = {}
    for scale in case['scales']:
        modes = [n for n, _ in variants]+['mygo']
        # Four phases, enlarged with nearest filtering so the viewer sees actual pixels.
        zoom, width, height = 8, round(155*scale), round(28*scale)
        sheet = cairo.ImageSurface(cairo.FORMAT_ARGB32, width*zoom, (height*zoom+24)*len(modes))
        ctx = cairo.Context(sheet)
        ctx.set_source_rgb(.93, .93, .93)
        ctx.paint()
        for index, mode in enumerate(modes):
            path = out/f'mygo-slight-{scale}x.png' if mode == 'mygo' else out/mode/f'plico-text-quality-{scale}x.png'
            image = cairo.ImageSurface.create_from_png(str(path))
            y = index*(height*zoom+24)
            ctx.set_source_rgb(0, 0, 0)
            ctx.move_to(8, y+17)
            ctx.show_text(f'{mode} / 14px 400 / {scale}x')
            ctx.save()
            ctx.rectangle(0, y+24, width*zoom, height*zoom)
            ctx.clip()
            ctx.translate(0, y+24)
            ctx.scale(zoom, zoom)
            ctx.set_source_surface(image, -round(15*scale), -round(52*scale))
            ctx.get_source().set_filter(cairo.FILTER_NEAREST)
            ctx.paint()
            ctx.restore()
            data, stride = bytes(image.get_data()), image.get_stride()
            groups = []
            for start in range(0, len(rows), 8):
                coverage = []
                for row in rows[start:start+8]:
                    x, baseline = row['x']*scale, row['baseline']*scale
                    coverage.append(sum(255-data[yy*stride+xx*4]
                        for yy in range(int(baseline-25*scale), int(baseline+4*scale))
                        for xx in range(int(x-3*scale), int(x+20*scale))))
                groups.append(dict(size=rows[start]['size'], weight=rows[start]['weight'],
                    coverage=coverage, spread_percent=100*(max(coverage)-min(coverage))/(sum(coverage)/8)))
            stats[f'{mode}-{scale}'] = groups
        sheet.write_to_png(str(out/f's-focused-{scale}x.png'))
    (out/'phase-coverage.json').write_text(json.dumps(stats, indent=2)+'\n')
    print(out)


if __name__ == '__main__':
    main()
