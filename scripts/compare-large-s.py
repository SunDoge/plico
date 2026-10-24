#!/usr/bin/env python3
"""Compare large s terminals against identical precise Cairo outlines."""
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import cairo


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=Path('reports/large-s'))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    out = (root/args.output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    helper = runpy.run_path(str(root/'scripts/compare-s-rasterizers.py'))
    subprocess.run(['cargo','build','--locked','--offline','-p','plico-shell',
                    '--example','text_quality_probe'],cwd=root,check=True)
    meta = json.loads(subprocess.check_output(['cargo','metadata','--offline','--no-deps','--format-version=1'],cwd=root))
    binary = Path(meta['target_directory'])/'debug/examples/text_quality_probe'
    env = os.environ.copy()
    env.pop('PLICO_TEXT_STEM_BOOST',None)
    records = {}
    for family in ('Noto Sans','DejaVu Sans','Liberation Sans'):
        directory = out/family.replace(' ','-')
        directory.mkdir(exist_ok=True)
        case = dict(width=200,height=520,background='#ffffff',foreground='#000000',
                    fontFamily=family,scales=[1],rows=[
                        dict(text='s',size=size,weight=400,x=25,baseline=base)
                        for size,base in ((32,55),(64,145),(128,290),(256,500))])
        manifest=directory/'case.json'
        manifest.write_text(json.dumps(case,indent=2)+'\n')
        images=[]
        for mode in ('lcd','lcd-flat','none'):
            d=directory/mode
            flags=['--outline-mode','none' if mode=='none' else 'lcd','--export-outlines']
            if mode=='lcd-flat': flags+=['--outline-flatness','0.005']
            with (directory/f'{mode}.log').open('w') as log:
                subprocess.run([str(binary),'--cases',str(manifest),'--output',str(d),*flags],
                               cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
            im=cairo.ImageSurface.create_from_png(str(d/'plico-text-quality-1x.png'))
            images.append(im)
            paths=json.loads((d/'outlines-1x.json').read_text())
            if mode == 'none':
                commands = []
                for path in paths:
                    commands.append(' '.join(('M','L','Q','C','Z')[int(e[0])]+' '.join(map(str,e[1:])) for e in path))
                svg = '<svg xmlns="http://www.w3.org/2000/svg" width="200" height="520" viewBox="0 0 200 520"><rect width="200" height="520" fill="white"/>'
                svg += ''.join('<path fill="black" d="'+command+'"/>' for command in commands)+'</svg>'
                (directory/'unhinted-outline.svg').write_text(svg)
            mask=helper['coverage'](paths,200,520,1)
            reference=helper['image'](mask)
            reference.write_to_png(str(d/'cairo.png'))
            raw,stride=bytes(im.get_data()),im.get_stride()
            delta=[abs(255-raw[y*stride+x*4]-mask[y][x])
                   for y in range(520) for x in range(200)
                   if mask[y][x] or raw[y*stride+x*4]!=255]
            records[f'{family}-{mode}']=dict(max_delta=max(delta),mean_ink_delta=sum(delta)/len(delta))
        # Full unhinted path in Cairo distinguishes geometry from hinting/renderer.
        images.append(cairo.ImageSurface.create_from_png(str(directory/'none/cairo.png')))
        sheet=cairo.ImageSurface(cairo.FORMAT_ARGB32,800,550)
        ctx=cairo.Context(sheet)
        ctx.set_source_rgb(.93,.93,.93)
        ctx.paint()
        for index,(label,im) in enumerate(zip(('Vello default','Vello fine flatten','Vello no hint','Cairo no hint'),images)):
            ctx.set_source_rgb(0,0,0)
            ctx.move_to(index*200+8,18)
            ctx.show_text(label)
            ctx.set_source_surface(im,index*200,30)
            ctx.rectangle(index*200,30,200,520)
            ctx.fill()
        sheet.write_to_png(str(directory/'comparison.png'))
    (out/'summary.json').write_text(json.dumps(records,indent=2)+'\n')
    print(out)


if __name__=='__main__':
    main()
