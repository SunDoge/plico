#!/usr/bin/env python3
"""Rasterize identical physical Skrifa s paths with Vello and Cairo.

Cairo also draws the exact same path at 8x then box-filters coverage, without
reshaping or rerunning hinting at the larger size. Requires Pycairo.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import cairo


def draw_paths(ctx, paths):
    for path in paths:
        ctx.new_path()
        for e in path:
            kind, *p = e
            if kind == 0:
                ctx.move_to(*p)
            elif kind == 1:
                ctx.line_to(*p)
            elif kind == 2:
                x, y = ctx.get_current_point()
                a, b, c, d = p
                ctx.curve_to(x+2*(a-x)/3, y+2*(b-y)/3,
                             c+2*(a-c)/3, d+2*(b-d)/3, c, d)
            elif kind == 3:
                ctx.curve_to(*p)
            else:
                ctx.close_path()
        ctx.fill()


def coverage(paths, w, h, factor):
    surface = cairo.ImageSurface(cairo.FORMAT_A8, w*factor, h*factor)
    ctx = cairo.Context(surface)
    ctx.scale(factor, factor)
    ctx.set_tolerance(0.001)
    ctx.set_source_rgba(1, 1, 1, 1)
    draw_paths(ctx, paths)
    data, stride = bytes(surface.get_data()), surface.get_stride()
    if factor == 1:
        return [list(data[y*stride:y*stride+w]) for y in range(h)]
    return [[round(sum(data[(y*factor+dy)*stride+x*factor+dx]
                       for dy in range(factor) for dx in range(factor))/(factor*factor))
             for x in range(w)] for y in range(h)]


def image(mask):
    h, w = len(mask), len(mask[0])
    s = cairo.ImageSurface(cairo.FORMAT_ARGB32, w, h)
    data = s.get_data()
    for y, row in enumerate(mask):
        for x, value in enumerate(row):
            p = y*s.get_stride()+x*4
            data[p:p+4] = bytes([255-value]*3+[255])
    s.mark_dirty()
    return s


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=Path, default=Path('reports/s-rasterizers'))
    args = p.parse_args()
    root = Path(__file__).resolve().parents[1]
    out = (root/args.output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    subprocess.run(['cargo', 'build', '--locked', '--offline', '-p', 'plico-shell',
                    '--example', 'text_quality_probe'], cwd=root, check=True)
    meta = json.loads(subprocess.check_output(['cargo', 'metadata', '--offline', '--no-deps', '--format-version=1'], cwd=root))
    binary = Path(meta['target_directory'])/'debug/examples/text_quality_probe'
    # Integer physical baselines; half-pixel and integer x phases.
    case = dict(width=110, height=120, background='#ffffff', foreground='#000000',
                fontFamily='Noto Sans', scales=[1, 1.75], rows=[
                    dict(text='s', size=size, weight=400, x=15+phase*35+phase*.5,
                         baseline=30+i*32)
                    for i, size in enumerate((12, 14, 16)) for phase in (0, 1)])
    manifest = out/'case.json'
    manifest.write_text(json.dumps(case, indent=2)+'\n')
    env = os.environ.copy()
    env.pop('PLICO_TEXT_STEM_BOOST', None)
    summary = {}
    for mode in ('lcd', 'lcd-flat', 'none'):
        directory = out/mode
        with (out/f'{mode}.log').open('w') as log:
            subprocess.run([str(binary), '--cases', str(manifest), '--output', str(directory),
                            '--outline-mode', 'lcd' if mode == 'lcd-flat' else mode, '--export-outlines',
                            *(['--outline-flatness', '0.005'] if mode == 'lcd-flat' else [])], env=env, cwd=root,
                           stdout=log, stderr=subprocess.STDOUT, check=True)
        for scale in case['scales']:
            native = cairo.ImageSurface.create_from_png(str(directory/f'plico-text-quality-{scale}x.png'))
            w, h = native.get_width(), native.get_height()
            paths = json.loads((directory/f'outlines-{scale}x.json').read_text())
            assert len(paths) == len(case['rows']), 'one exported physical path per s'
            masks = [coverage(paths, w, h, f) for f in (1, 8)]
            data, stride = bytes(native.get_data()), native.get_stride()
            vello = [[255-data[y*stride+x*4] for x in range(w)] for y in range(h)]
            all_masks = [vello, *masks]
            records = []
            for index, row in enumerate(case['rows']):
                x, y = row['x']*scale, row['baseline']*scale
                x0, x1 = max(0, int(x-3*scale)), min(w, int(x+20*scale))
                y0, y1 = max(0, int(y-24*scale)), min(h, int(y+4*scale))
                profiles = [[sum(m[yy][x0:x1])/255 for yy in range(y0, y1)] for m in all_masks]
                errors = []
                for m in masks:
                    delta = [abs(vello[yy][xx]-m[yy][xx]) for yy in range(y0,y1) for xx in range(x0,x1)
                             if vello[yy][xx] or m[yy][xx]]
                    errors.append(dict(max_coverage_delta=max(delta), mean_ink_delta=sum(delta)/len(delta)))
                records.append(dict(size=row['size'], x=row['x'], profiles=profiles, comparisons=errors, ink_sums=[sum(profile) for profile in profiles]))
            summary[f'{mode}-{scale}x'] = records
            images = [native, *[image(m) for m in masks]]
            for factor, im in zip((1, 8), images[1:]):
                im.write_to_png(str(directory/f'cairo-{factor}x-coverage-{scale}x.png'))
            zoom = 6
            sheet = cairo.ImageSurface(cairo.FORMAT_ARGB32, w*zoom, (h*zoom+25)*3)
            ctx = cairo.Context(sheet)
            ctx.set_source_rgb(.93,.93,.93)
            ctx.paint()
            for i,(label,im) in enumerate(zip(('Vello same physical outline','Cairo same physical outline','Cairo 8x then box average; SAME outline, NO rehint'), images)):
                top = i*(h*zoom+25)
                ctx.set_source_rgb(0,0,0)
                ctx.move_to(8,top+17)
                ctx.show_text(label)
                ctx.save()
                ctx.rectangle(0,top+25,w*zoom,h*zoom)
                ctx.clip()
                ctx.translate(0,top+25)
                ctx.scale(zoom,zoom)
                ctx.set_source_surface(im,0,0)
                ctx.get_source().set_filter(cairo.FILTER_NEAREST)
                ctx.paint()
                ctx.restore()
            sheet.write_to_png(str(directory/f'comparison-{scale}x.png'))
    # Compare original and refined Vello output against ONE original-path reference.
    common = {}
    for scale in case['scales']:
        sources = [out/'lcd'/f'plico-text-quality-{scale}x.png',
                   out/'lcd-flat'/f'plico-text-quality-{scale}x.png',
                   out/'lcd'/f'cairo-8x-coverage-{scale}x.png']
        surfaces = [cairo.ImageSurface.create_from_png(str(p)) for p in sources]
        x0, y0 = round(12*scale), round(44*scale)
        width, height, zoom = round(58*scale), round(24*scale), 12
        sheet = cairo.ImageSurface(cairo.FORMAT_ARGB32, width*zoom, (height*zoom+24)*3)
        ctx = cairo.Context(sheet)
        ctx.set_source_rgb(.93,.93,.93)
        ctx.paint()
        samples = []
        for i,(label,im) in enumerate(zip(('Vello default flattening','Vello preflatten 0.005px','Original exact path: Cairo 8x area reference'), surfaces)):
            raw, stride = bytes(im.get_data()), im.get_stride()
            samples.append([255-raw[y*stride+x*4] for y in range(y0,y0+height) for x in range(x0,x0+width)])
            top = i*(height*zoom+24)
            ctx.set_source_rgb(0,0,0)
            ctx.move_to(8,top+17)
            ctx.show_text(label+f' / 14px / {scale}x')
            ctx.save()
            ctx.rectangle(0,top+24,width*zoom,height*zoom)
            ctx.clip()
            ctx.translate(0,top+24)
            ctx.scale(zoom,zoom)
            ctx.set_source_surface(im,-x0,-y0)
            ctx.get_source().set_filter(cairo.FILTER_NEAREST)
            ctx.paint()
            ctx.restore()
        sheet.write_to_png(str(out/f'before-after-{scale}x.png'))
        errors = []
        for sample in samples[:2]:
            delta = [abs(a-b) for a,b in zip(sample,samples[2]) if a or b]
            errors.append(dict(max_coverage_delta=max(delta),mean_ink_delta=sum(delta)/len(delta)))
        common[str(scale)] = dict(default=errors[0],refined=errors[1])
    (out/'common-reference.json').write_text(json.dumps(common,indent=2)+'\n')
    (out/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    print(out)


if __name__ == '__main__':
    main()
