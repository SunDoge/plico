#!/usr/bin/env python3
"""Fit one coverage exponent on Latin 12px/1x; validate other sizes/fonts/DPIs."""
import argparse
import json
import operator
from pathlib import Path
import sys
import cairo

ROOT = Path(__file__).resolve().parent.parent


def rows(path, scale, shifts=None):
    image = cairo.ImageSurface.create_from_png(str(path))
    data = bytes(image.get_data())
    width, stride = image.get_width(), image.get_stride()
    channel = 0 if sys.byteorder == "little" else 1
    result = []
    for i in range(24):
        y0, y1 = round(i*48*scale), round((i+1)*48*scale)
        row = [data[y*stride+channel:y*stride+width*4:4] for y in range(y0,y1)]
        dx, dy = (shifts[i]["dx"], shifts[i]["dy"]) if shifts else (0,0)
        shifted = []
        for y in range(len(row)):
            line = row[y-dy] if 0 <= y-dy < len(row) else bytes(width)
            line = bytes(dx)+line[:width-dx] if dx>0 else line[-dx:]+bytes(-dx) if dx<0 else line
            shifted.append(line)
        result.append(b"".join(shifted))
    return result


def lut(exponent):
    return bytes(round(255*(i/255)**exponent) for i in range(256))


def error(reference, candidate):
    return sum(map(abs,map(operator.sub,reference,candidate)))


def transformed(source, table, destination):
    image = cairo.ImageSurface.create_from_png(str(source))
    original = bytes(image.get_data())
    data = bytearray(original)
    channels = (0,1,2) if sys.byteorder == "little" else (1,2,3)
    for channel in channels:
        data[channel::4] = original[channel::4].translate(table)
    out = cairo.ImageSurface.create_for_data(data,cairo.FORMAT_ARGB32,image.get_width(),image.get_height(),image.get_stride())
    out.write_to_png(str(destination))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source",type=Path,default=ROOT/"reports/hint-targets")
    parser.add_argument("--chrome",type=Path,default=ROOT/"reports/chrome-text")
    parser.add_argument("--output",type=Path,default=ROOT/"reports/coverage-analysis")
    args=parser.parse_args()
    source, chrome, out=args.source.resolve(), args.chrome.resolve(), args.output.resolve()
    out.mkdir(parents=True,exist_ok=True)
    m=json.loads((chrome/"measurements.json").read_text())
    train=m["results"]["latin"]["1"]
    ref=rows(chrome/"latin/1/chrome.png",1)
    candidate=rows(source/"latin/1/ft-full-aligned-cairo.png",1,train["alignment"]["ft_full_aligned"]["rows"])
    ref_train=b"".join(ref[:8]); cand_train=b"".join(candidate[:8])
    exponent=min((i/100 for i in range(60,141)),key=lambda p:error(ref_train,cand_train.translate(lut(p))))
    table=lut(exponent)
    results={}
    for font, scales in m["results"].items():
        results[font]={}
        for scale, record in scales.items():
            scale_value=float(scale)
            folder=source/font/scale
            reference=rows(chrome/font/scale/"chrome.png",scale_value)
            result={}
            for variant in ("vello_hint","skrifa_lcd-symmetric","ft_full_aligned"):
                path=folder/"ft-full-aligned-cairo.png" if variant.startswith("ft_") else folder/variant/f"plico-text-quality-{scale}x.png"
                candidate=rows(path,scale_value,record["alignment"][variant]["rows"])
                by_size={}
                for group,size in enumerate((12,14,16)):
                    ref_group=b"".join(reference[group*8:(group+1)*8])
                    cand_group=b"".join(candidate[group*8:(group+1)*8])
                    mass=sum(ref_group)
                    by_size[str(size)]=dict(before_percent=error(ref_group,cand_group)/mass*100,
                        after_percent=error(ref_group,cand_group.translate(table))/mass*100,
                        mass_ratio_before=sum(cand_group)/mass,mass_ratio_after=sum(cand_group.translate(table))/mass)
                result[variant]=by_size
                transformed(path,table,out/f"{font}-{scale}-{variant}.png")
            results[font][scale]=result
    (out/"measurements.json").write_text(json.dumps(dict(training="Latin 12px 1x; same font/geometry; fixed previously computed integer alignment",exponent=exponent,results=results),indent=2))
    print("Fitted exponent:",exponent)
    for font,scales in results.items():
        for scale,r in scales.items():
            print(font,scale,"14px",{k:(round(v["14"]["before_percent"],2),round(v["14"]["after_percent"],2)) for k,v in r.items()})


if __name__=="__main__":
    main()
