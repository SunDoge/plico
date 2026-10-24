#!/usr/bin/env python3
"""Build actual-size small-text specimens using Plico theme colors and system fallback."""
import argparse
import base64
import json
import os
from pathlib import Path
import runpy
import subprocess

import cairo

ROOT = Path(__file__).resolve().parent.parent


def write_review(out):
    # Self-contained so app/browser HTML previews need no access to sibling files.
    images = {}
    for theme in ('dark','light'):
        for scale in (1,1.75):
            for name in ('system-default','noto-default','noto-coarse-embolden','noto-coverage'):
                filename='coverage-preview.png' if name=='noto-coverage' else f'plico-text-quality-{scale}x.png'
                path=f'{theme}/{scale}/{name}/{filename}'
                images[path]='data:image/png;base64,'+base64.b64encode((out/path).read_bytes()).decode('ascii')
    template=(ROOT/'scripts/text-comparison.html').read_text()
    (out/'review.html').write_text(template.replace('const embeddedImages = {};',
        'const embeddedImages = '+json.dumps(images)+';',1))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'reports/small-text')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    variants = [('system-default', 'sans-serif', None, []),
                ('noto-default', 'Noto Sans, Noto Sans CJK SC, sans-serif', None, []),
                ('noto-coarse-embolden', 'Noto Sans, Noto Sans CJK SC, sans-serif', 2,
                 ['--outline-mode','lcd','--embolden-tolerance','0.1']),
                ('noto-no-boost', 'Noto Sans, Noto Sans CJK SC, sans-serif', 0, []),
                ('noto-coverage', 'Noto Sans, Noto Sans CJK SC, sans-serif', 0, []),
                ('noto-boost4', 'Noto Sans, Noto Sans CJK SC, sans-serif', 4, []),
                ('noto-no-hint', 'Noto Sans, Noto Sans CJK SC, sans-serif', 0, ['--no-hint'])]
    for theme, fg, bg in [('light', '#202024', '#f7f7f8'), ('dark', '#f4f4f5', '#18181b')]:
        for scale in (1, 1.75):
            images = []
            for name, family, boost, flags in variants:
                folder = out / theme / str(scale) / name
                folder.mkdir(parents=True, exist_ok=True)
                rows = []
                for weight in (400, 500):
                    for size in (12, 14, 16):
                        rows.append(dict(text='中文字体渲染：设置、连接、保存 / Hello Rust 0123',
                                         size=size, weight=weight, x=20, baseline=32+len(rows)*40))
                case = folder / 'case.json'
                case.write_text(json.dumps(dict(width=660, height=260, background=bg,
                    foreground=fg, fontFamily=family, scales=[scale], rows=rows), ensure_ascii=False))
                environment={key:value for key,value in os.environ.items() if key!='PLICO_TEXT_STEM_BOOST'}
                if boost is not None:
                    environment['PLICO_TEXT_STEM_BOOST']=str(boost)
                subprocess.run(['cargo','run','--locked','--offline','-p','plico-shell',
                    '--example','text_quality_probe','--','--cases',str(case),
                    '--output',str(folder),*flags], cwd=ROOT, check=True,
                    env=environment, stdout=subprocess.DEVNULL)
                image = cairo.ImageSurface.create_from_png(str(folder/f'plico-text-quality-{scale}x.png'))
                if name == 'noto-coverage':
                    # Diagnostic text-only postprocess, not a production renderer.
                    # This fixture has just opaque text over one constant background.
                    helper = runpy.run_path(str(ROOT/'scripts/analyze-text-colors.py'))
                    foreground = bytes.fromhex(fg[1:])
                    background = bytes.fromhex(bg[1:])
                    # Frozen black/white training fit; do not refit these previews.
                    table = helper['transfer'](foreground[0],1.2,.2)
                    original = bytes(image.get_data())
                    data = bytearray(original)
                    for offset in range(0,len(data),4):
                        a = round(255*(original[offset+2]-background[0])/(foreground[0]-background[0]))
                        coverage = table[max(0,min(255,a))]/255
                        for channel in range(3):
                            data[offset+2-channel] = round(background[channel]+coverage*(foreground[channel]-background[channel]))
                    image = cairo.ImageSurface.create_for_data(data,cairo.FORMAT_ARGB32,
                        image.get_width(),image.get_height(),image.get_stride())
                    image.write_to_png(str(folder/'coverage-preview.png'))
                images.append((name,image))
            width, height = images[0][1].get_width(), images[0][1].get_height()
            sheet = cairo.ImageSurface(cairo.FORMAT_ARGB32, width, (height+32)*len(images))
            context = cairo.Context(sheet)
            context.set_source_rgb(.15,.15,.15)
            context.paint()
            for index,(name,image) in enumerate(images):
                y = index*(height+32)
                context.set_source_rgb(1,1,1)
                context.set_font_size(16)
                context.move_to(12,y+23)
                context.show_text(f'{name}: 12/14/16px regular, then medium; {scale}x')
                context.set_source_surface(image,0,y+32)
                context.paint()
            path = out / f'{theme}-{scale}x.png'
            sheet.write_to_png(str(path))
            print(path, flush=True)
    # A compact original-physical-size specimen for visual review; no resampling.
    selections = [(theme,name) for theme in ('dark','light')
                  for name in ('system-default','noto-default','noto-coarse-embolden','noto-coverage')]
    for size,scale in ((12,1),(12,1.75),(14,1.75)):
        width, row_height = round(660*scale), round(40*scale)
        band_height = row_height+32
        sheet = cairo.ImageSurface(cairo.FORMAT_ARGB32,width,len(selections)*band_height)
        context = cairo.Context(sheet)
        context.set_source_rgb(.15,.15,.15)
        context.paint()
        for index,(theme,name) in enumerate(selections):
            filename = 'coverage-preview.png' if name=='noto-coverage' else f'plico-text-quality-{scale}x.png'
            image = cairo.ImageSurface.create_from_png(str(out/theme/str(scale)/name/filename))
            y = index*band_height
            context.set_source_rgb(1,1,1)
            context.set_font_size(16)
            context.move_to(12,y+22)
            context.show_text(f'{theme} {name} / {size}px regular / {scale}x')
            context.save()
            context.rectangle(0,y+32,width,row_height)
            context.clip()
            context.set_source_surface(image,0,y+32-(0 if size==12 else row_height))
            context.paint()
            context.restore()
        filename = 'focused-14px.png' if size==14 else f'focused-{size}px-{scale}x.png'
        sheet.write_to_png(str(out/filename))
    write_review(out)
    print(out/'review.html')
    # Same-font before/after: isolate removal of the old extra expansion.
    sheet=cairo.ImageSurface(cairo.FORMAT_ARGB32,1155,4*104)
    context=cairo.Context(sheet)
    context.set_source_rgb(.15,.15,.15)
    context.paint()
    for index,(theme,old) in enumerate((t,b) for t in ('dark','light') for b in (True,False)):
        name='noto-coarse-embolden' if old else 'noto-default'
        image=cairo.ImageSurface.create_from_png(str(out/theme/'1.75'/name/'plico-text-quality-1.75x.png'))
        y=index*104
        context.set_source_rgb(1,1,1)
        context.set_font_size(16)
        context.move_to(12,y+23)
        context.show_text(f'{theme.title()} / '+('Before' if old else 'After')+' / 14px at 1.75x')
        context.save()
        context.rectangle(0,y+32,1155,70)
        context.clip()
        context.set_source_surface(image,0,y+32-70)
        context.paint()
        context.restore()
    sheet.write_to_png(str(out/'before-after.png'))


if __name__ == '__main__':
    main()
