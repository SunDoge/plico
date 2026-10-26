#!/usr/bin/env python3
"""Scan 8x8 physical subpixel phases; compare regional s coverage and history.

No font/shape changes between rasterizer variants. Bands follow the actual
outline bounding box, with fractional pixel overlap at band boundaries.
"""
import argparse
import json
import math
import os
from pathlib import Path
import runpy
import subprocess
import cairo


class Bounds:
    def __init__(self):
        self.ctx = cairo.Context(cairo.RecordingSurface(cairo.CONTENT_ALPHA, None))
        self.value = None
    def __getattr__(self, name):
        return getattr(self.ctx, name)
    def fill(self):
        self.value = self.ctx.path_extents()
        self.ctx.new_path()


def mask_from_png(path):
    im = cairo.ImageSurface.create_from_png(str(path))
    raw, stride = bytes(im.get_data()), im.get_stride()
    return [[255-raw[y*stride+x*4] for x in range(im.get_width())]
            for y in range(im.get_height())]


def bands(mask, box):
    left, top, right, bottom = box
    edges = [top, top+(bottom-top)*.3, top+(bottom-top)*.7, bottom]
    values = []
    for low, high in zip(edges, edges[1:]):
        # Include antialiasing immediately outside the outer outline bounds.
        if low == top: low = math.floor(top)
        if high == bottom: high = math.ceil(bottom)
        total = 0.
        for y in range(math.floor(low), math.ceil(high)):
            overlap = max(0., min(y+1, high)-max(y, low))
            total += overlap*sum(mask[y][math.floor(left):math.ceil(right)])/255
        values.append(total)
    return values


def terminals(mask, box):
    left, top, right, bottom = box
    width, height = right-left, bottom-top
    boxes = [(left+.65*width, top, right, top+.3*height),
             (left, top+.7*height, left+.35*width, bottom)]
    values = []
    for x0,y0,x1,y1 in boxes:
        total = 0.
        for y in range(math.floor(y0), math.ceil(y1)):
            wy = max(0., min(y+1,y1)-max(y,y0))
            for x in range(math.floor(x0), math.ceil(x1)):
                wx = max(0., min(x+1,x1)-max(x,x0))
                total += wx*wy*mask[y][x]/255
        values.append(total)
    return values


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=Path, default=Path('reports/s-phase-scan'))
    args = p.parse_args()
    root = Path(__file__).resolve().parents[1]
    out = (root/args.output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    helper = runpy.run_path(str(root/'scripts/compare-s-rasterizers.py'))
    subprocess.run(['cargo','build','--locked','--offline','-p','plico-shell',
                    '--example','text_quality_probe'],cwd=root,check=True)
    meta=json.loads(subprocess.check_output(['cargo','metadata','--offline','--no-deps','--format-version=1'],cwd=root))
    binary=Path(meta['target_directory'])/'debug/examples/text_quality_probe'
    env=os.environ.copy()
    env.pop('PLICO_TEXT_STEM_BOOST',None)
    report={}
    for size in (14,64):
        for scale in (1,1.75):
            directory=out/f'{size}px-{scale}x'
            directory.mkdir(exist_ok=True)
            rows=[dict(text='s',size=size,weight=400,
                       x=px*96+16+px/(8*scale),baseline=py*104+88+py/(8*scale))
                  for py in range(8) for px in range(8)]
            case=dict(width=768,height=832,background='#ffffff',foreground='#000000',
                      fontFamily='Noto Sans',scales=[scale],rows=rows)
            manifest=directory/'case.json'
            manifest.write_text(json.dumps(case,indent=2)+'\n')
            variants={}
            for name,flags in (
                ('native',['--check-artifacts']),
                ('hinted',['--outline-mode','lcd','--export-outlines']),
                ('fine',['--outline-mode','lcd','--outline-flatness','0.005','--export-outlines']),
                ('unhinted',['--outline-mode','none','--export-outlines'])):
                d=directory/name
                with (directory/f'{name}.log').open('w') as log:
                    subprocess.run([str(binary),'--cases',str(manifest),'--output',str(d),*flags],cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
                mask=mask_from_png(d/f'plico-text-quality-{scale}x.png')
                if name=='native':
                    native=mask
                    continue
                paths=json.loads((d/f'outlines-{scale}x.json').read_text())
                assert len(paths)==64
                boxes=[]
                for path in paths:
                    b=Bounds()
                    helper['draw_paths'](b,[path])
                    boxes.append(b.value)
                values=[bands(mask,b) for b in boxes]
                entry=dict(regional_coverage=values, terminal_coverage=[terminals(mask,b) for b in boxes])
                # Fine outlines are an experimental reference, not production Glifo.
                entry['different_pixels_vs_native'] = sum(
                    a != b for row_a, row_b in zip(native, mask)
                    for a, b in zip(row_a, row_b))
                if name=='hinted':
                    assert native == mask, 'production glyph path differs from upstream hinted outline'
                    reference=helper['coverage'](paths,len(mask[0]),len(mask),1)
                    helper['image'](reference).write_to_png(str(directory/'cairo.png'))
                    variants['cairo']=dict(regional_coverage=[bands(reference,b) for b in boxes], terminal_coverage=[terminals(reference,b) for b in boxes])
                    ref_values=variants['cairo']['regional_coverage']
                entry['errors_vs_hinted_cairo']=[
                    [100*(a-b)/b for a,b in zip(v,r)] for v,r in zip(values,ref_values)] if name!='unhinted' else None
                variants[name]=entry
            for name,entry in variants.items():
                values=entry['regional_coverage']
                entry['terminal_spread_percent']=[100*(max(col)-min(col))/(sum(col)/len(col)) for col in zip(*entry['terminal_coverage'])]
                if name != 'unhinted':
                    errs = [[100*(a-b)/b for a,b in zip(v,r)] for v,r in zip(entry['terminal_coverage'],variants['cairo']['terminal_coverage'])]
                    entry['terminal_error_ranges_percent'] = [[min(col),max(col)] for col in zip(*errs)]
                entry['band_spread_percent']=[100*(max(col)-min(col))/(sum(col)/len(col)) for col in zip(*values)]
                entry['upper_lower_ratio_range']=[min(v[0]/v[2] for v in values),max(v[0]/v[2] for v in values)]
                if entry.get('errors_vs_hinted_cairo'):
                    entry['band_error_ranges_percent']=[[min(col),max(col)] for col in zip(*entry['errors_vs_hinted_cairo'])]
            report[f'{size}px-{scale}x']=variants
            # Pick greatest default local-band error; compare exactly the same phase.
            errors=variants['hinted']['errors_vs_hinted_cairo']
            index=max(range(64),key=lambda i:max(abs(v) for v in errors[i]))
            px,py=index%8,index//8
            sources=[directory/'native'/f'plico-text-quality-{scale}x.png',directory/'fine'/f'plico-text-quality-{scale}x.png',directory/'cairo.png']
            tile_w,tile_h=round(80*scale),round(90*scale)
            zoom=6 if size==14 else 3
            sheet=cairo.ImageSurface(cairo.FORMAT_ARGB32,tile_w*zoom,(tile_h*zoom+24)*3)
            ctx=cairo.Context(sheet)
            ctx.set_source_rgb(.93,.93,.93);ctx.paint()
            for i,(name,path) in enumerate(zip(('Native','Fine flatten','Cairo same hinted path'),sources)):
                im=cairo.ImageSurface.create_from_png(str(path));y=i*(tile_h*zoom+24)
                ctx.set_source_rgb(0,0,0);ctx.move_to(6,y+17);ctx.show_text(f'{name}: phase ({px}/8,{py}/8), {size}px @ {scale}x')
                ctx.save();ctx.rectangle(0,y+24,tile_w*zoom,tile_h*zoom);ctx.clip()
                ctx.translate(0,y+24);ctx.scale(zoom,zoom)
                ctx.set_source_surface(im,-round(px*96*scale),-round((py*104+10)*scale))
                ctx.get_source().set_filter(cairo.FILTER_NEAREST);ctx.paint();ctx.restore()
            sheet.write_to_png(str(directory/'worst-phase.png'))
            print(f'passed {size}px @ {scale}x',flush=True)
    (out/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    print(out)


if __name__=='__main__':
    main()
