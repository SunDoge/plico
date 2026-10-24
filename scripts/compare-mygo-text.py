#!/usr/bin/env python3
"""Compare local MyGO Pango/atlas/software output with Plico's GPU output.

Uses a Go overlay; never writes into the MyGO checkout. Requires Pycairo and
installed Noto Sans / Noto Sans CJK SC. Reports are disposable.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import cairo


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--mygo', type=Path, default=Path('/home/me/code/go/mygo'))
    p.add_argument('--output', type=Path, default=Path('reports/mygo-text'))
    args = p.parse_args()
    root = Path(__file__).resolve().parents[1]
    out = (root / args.output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    mygo = args.mygo.resolve()
    overlay = out / 'overlay.json'
    overlay.write_text(json.dumps({'Replace': {
        str(mygo / 'internal/raster/plico_comparison_test.go'):
        str(root / 'scripts/mygo-text-probe.go.txt')}}))
    subprocess.run(['cargo', 'build', '--locked', '--offline', '-p', 'plico-shell',
                    '--example', 'text_quality_probe'], cwd=root, check=True)
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--no-deps', '--format-version=1', '--offline'], cwd=root))
    binary = Path(metadata['target_directory']) / 'debug/examples/text_quality_probe'
    env = os.environ.copy()
    env.pop('PLICO_TEXT_STEM_BOOST', None)
    env['CGO_ENABLED'] = '0'
    records = {}
    for theme, bg, fg in [('light', '#ffffff', '#18181b'), ('dark', '#18181b', '#f4f4f5')]:
        for name, family, text in [
            ('latin', 'Noto Sans', 'Hi ill .,:; 0123 MW / Save settings'),
            ('cjk', 'Noto Sans CJK SC', '口目日田中 设置连接保存 ：，。密集笔画'),
            ('mixed', 'Noto Sans, Noto Sans CJK SC, sans-serif', '中文字体渲染：设置 / Hello Rust .,:; 0123')]:
            d = out / f'{name}-{theme}'
            d.mkdir(exist_ok=True)
            case = dict(width=660, height=260, background=bg, foreground=fg,
                        fontFamily=family, scales=[1, 1.75], rows=[
                            dict(text=text, size=size, weight=weight, x=20, baseline=32+i*40)
                            for i, (weight, size) in enumerate((w, s) for w in (400, 500) for s in (12, 14, 16))])
            manifest = d / 'case.json'
            manifest.write_text(json.dumps(case, ensure_ascii=False, indent=2)+'\n')
            with (d / 'plico.log').open('w') as log:
                subprocess.run([str(binary), '--cases', str(manifest), '--output', str(d)],
                               env=env, cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)
            for hint in ('slight', 'medium'):
                ge = dict(env, PLICO_MYGO_CASE=str(manifest), PLICO_MYGO_OUTPUT=str(d), PLICO_MYGO_HINT=hint)
                with (d / f'mygo-{hint}.log').open('w') as log:
                    subprocess.run(['go', 'test', '-overlay', str(overlay), './internal/raster',
                                    '-run', '^TestPlicoTextComparison$', '-count=1', '-v'],
                                   cwd=mygo, env=ge, stdout=log, stderr=subprocess.STDOUT, check=True)
            for scale in case['scales']:
                names = [f'plico-text-quality-{scale}x.png', f'mygo-slight-{scale}x.png', f'mygo-medium-{scale}x.png']
                images = [cairo.ImageSurface.create_from_png(str(d/n)) for n in names]
                w, h = images[0].get_width(), images[0].get_height()
                sheet = cairo.ImageSurface(cairo.FORMAT_ARGB32, w, (h+28)*3)
                draw = cairo.Context(sheet)
                draw.set_source_rgb(.93, .93, .93)
                draw.paint()
                for i, (label, image) in enumerate(zip(('Plico: production default', 'MyGO: grayscale / slight (GNOME configured hinting)', 'MyGO: grayscale / medium (standalone default)'), images)):
                    draw.set_source_rgb(0, 0, 0)
                    draw.move_to(8, i*(h+28)+18)
                    draw.show_text(label)
                    draw.set_source_surface(image, 0, i*(h+28)+28)
                    draw.paint()
                sheet.write_to_png(str(d / f'comparison-{scale}x.png'))
                # Compact A/B with the same physical-pixel size, 14px regular/medium.
                row_height = round(40*scale)
                focus = cairo.ImageSurface(cairo.FORMAT_ARGB32, w, (row_height+24)*6)
                ctx = cairo.Context(focus)
                ctx.set_source_rgb(.93, .93, .93)
                ctx.paint()
                for weight_index, row_index in enumerate((1, 4)):
                    for variant, (label, image) in enumerate(zip(('Plico', 'MyGO slight', 'MyGO medium'), images)):
                        y = (weight_index*3+variant)*(row_height+24)
                        ctx.set_source_rgb(0, 0, 0)
                        ctx.move_to(8, y+16)
                        ctx.show_text(f'{label} / 14px / weight {400+weight_index*100} / {scale}x')
                        ctx.save()
                        ctx.rectangle(0, y+24, w, row_height)
                        ctx.clip()
                        ctx.set_source_surface(image, 0, y+24-round((row_index*40+6)*scale))
                        ctx.paint()
                        ctx.restore()
                focus.write_to_png(str(d / f'focused-14px-{scale}x.png'))

                background = tuple(reversed(bytes.fromhex(bg[1:])))
                stats = []
                for image in images:
                    data = bytes(image.get_data())
                    pixels = [data[y*image.get_stride()+x*4:y*image.get_stride()+x*4+3]
                              for y in range(h) for x in range(w)]
                    stats.append(dict(ink_pixels=sum(tuple(p) != background for p in pixels),
                        contrast_sum=sum(max(abs(a-b) for a,b in zip(p, background)) for p in pixels)))
                records[f'{name}-{theme}-{scale}x'] = dict(zip(('plico', 'mygo_slight', 'mygo_medium'), stats))
    (out / 'summary.json').write_text(json.dumps(records, indent=2)+'\n')
    print(out)


if __name__ == '__main__':
    main()
