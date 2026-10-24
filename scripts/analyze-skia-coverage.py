#!/usr/bin/env python3
"""Separate quarter-pixel caching and color-dependent coverage transfer from shaping."""
import argparse
import json
import math
import operator
from pathlib import Path
import runpy
import sys
import cairo
from text_freetype import FreeType, composite_cairo

ROOT=Path(__file__).resolve().parent.parent
analyze=runpy.run_path(str(Path(__file__).with_name("analyze-text-coverage.py")))
phases=runpy.run_path(str(Path(__file__).with_name("compare-text-phases.py")))


def tables(gamma, contrast):
    # Limiting black/white cases of Skia's source-luminance-dependent mask model.
    # This fits a policy, not the browser's private runtime configuration.
    white=bytes(round(255*(a/255)**(1/gamma)) for a in range(256))
    black=bytes(round(255*(1-(1-(a/255+contrast*(a/255)*(1-a/255)))**(1/gamma))) for a in range(256))
    return white,black


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source",type=Path,default=ROOT/"reports/hint-targets-exact")
    parser.add_argument("--chrome",type=Path,default=ROOT/"reports/chrome-text-exact")
    parser.add_argument("--output",type=Path,default=ROOT/"reports/skia-coverage")
    args=parser.parse_args()
    source,chrome,out=args.source.resolve(),args.chrome.resolve(),args.output.resolve()
    out.mkdir(parents=True,exist_ok=True)
    ft=FreeType(out)
    caches={}
    for kind in ("latin","cjk"):
        caches[kind]={}
        for scale in (1,1.25,1.5,2):
            folder=source/kind/str(scale)
            case=json.loads((folder/"case.json").read_text())
            native=phases["support"]["native_metrics"](folder/"vello_hint"/f"plico-text-quality-{scale}x.txt")
            physical=json.loads((folder/"vello_hint"/f"plico-physical-glyphs-{scale}x.json").read_text())
            cursor=0
            for row in native:
                count=len(row["glyphs"]);row["physicalGlyphs"]=physical[cursor:cursor+count];cursor+=count
            cache={}
            for mode in ("slight","full"):
                path=out/f"{kind}-{scale}-{mode}-quarter.png"
                mask,_=ft.mask(case,scale,native,mode,out/f"{kind}-{scale}-{mode}-mask.png",snap_y=True,x_steps=4)
                composite_cairo(mask,case,path)
                cache[mode]=analyze["rows"](path,scale)
            cache["reference_white"]=analyze["rows"](chrome/kind/str(scale)/"chrome.png",scale)
            cache["reference_black"]=analyze["rows"](chrome.with_name(chrome.name+"-light")/kind/str(scale)/"chrome-coverage.png",scale)
            caches[kind][str(scale)]=cache
    # Fit both polarities using only 12px Latin at 1x. Everything else is held out.
    train=caches["latin"]["1"]
    candidate=b"".join(train["full"][:8])
    refs=[b"".join(train[f"reference_{polarity}"][:8]) for polarity in ("white","black")]
    error=analyze["error"]
    def loss(pair):
        ts=tables(*pair)
        return sum(error(ref,candidate.translate(table)) for ref,table in zip(refs,ts))
    gamma,contrast=min(((1+i*.05,j*.05) for i in range(21) for j in range(17)),key=loss)
    transfers=tables(gamma,contrast)
    results={}
    for kind,scales in caches.items():
        results[kind]={}
        for scale,cache in scales.items():
            result={}
            for mode in ("slight","full"):
                for polarity,transfer in zip(("white","black"),transfers):
                    by_size={}
                    for group,size in enumerate((12,14,16)):
                        reference=b"".join(cache[f"reference_{polarity}"][group*8:(group+1)*8])
                        candidate=b"".join(cache[mode][group*8:(group+1)*8])
                        mass=sum(reference)
                        by_size[str(size)]=dict(before_percent=error(reference,candidate)/mass*100,
                            after_percent=error(reference,candidate.translate(transfer))/mass*100,
                            mass_ratio_before=sum(candidate)/mass,mass_ratio_after=sum(candidate.translate(transfer))/mass)
                    result[f"{mode}_{polarity}"]=by_size
            results[kind][scale]=result
    ft.close()
    (out/"measurements.json").write_text(json.dumps(dict(training="Latin 12px 1x, both polarities, FreeType normal on quarter-pixel X / integer Y",gamma=gamma,contrast=contrast,results=results),indent=2))
    print("Fit gamma/contrast:",gamma,contrast)
    for kind,scales in results.items():
        for scale,r in scales.items():
            print(kind,scale,"14px",{k:(round(v["14"]["before_percent"],2),round(v["14"]["after_percent"],2)) for k,v in r.items()})


if __name__=="__main__":
    main()
