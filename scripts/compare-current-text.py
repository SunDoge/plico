#!/usr/bin/env python3
"""Measure the current default stem boost against the same Chrome references."""
import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess

ROOT=Path(__file__).resolve().parent.parent


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source",type=Path,default=ROOT/"reports/hint-targets-exact")
    parser.add_argument("--chrome",type=Path,default=ROOT/"reports/chrome-text-exact")
    parser.add_argument("--output",type=Path,default=ROOT/"reports/current-text")
    parser.add_argument("--scales",type=float,nargs='+',default=[1,1.25,1.5,2])
    args=parser.parse_args()
    source,chrome,out=args.source.resolve(),args.chrome.resolve(),args.output.resolve()
    out.mkdir(parents=True,exist_ok=True)
    tools=runpy.run_path(str(Path(__file__).with_name("compare-text-phases.py")))
    refs={polarity:json.loads((folder/"measurements.json").read_text()) for polarity,folder in
          [("white",chrome),("black",chrome.with_name(chrome.name+"-light"))]}
    results={}
    # Remove the override to test the actual compiled default, not a hand-picked value.
    env={k:v for k,v in os.environ.items() if k!="PLICO_TEXT_STEM_BOOST"}
    for kind in ("latin","cjk"):
        results[kind]={}
        for scale in args.scales:
            if float(scale).is_integer():
                scale=int(scale)
            case_path=source/kind/str(scale)/"case.json"
            case=json.loads(case_path.read_text())
            target=out/kind/str(scale)
            subprocess.run(["cargo","run","--locked","--offline","-p","plico-shell",
                "--example","text_quality_probe","--","--cases",str(case_path),
                "--output",str(target),"--exact-baselines"],cwd=ROOT,env=env,check=True,
                stdout=subprocess.DEVNULL)
            image=target/f"plico-text-quality-{scale}x.png"
            current=tools["stats"](image,case,scale)
            record={}
            for polarity,folder in [("white",chrome),("black",chrome.with_name(chrome.name+"-light"))]:
                reference=folder/kind/str(scale)/("chrome.png" if polarity=="white" else "chrome-coverage.png")
                record[polarity]=dict(coverage=tools["coverage_difference"](reference,image),
                    sizes={size:dict(ink_ratio=sample["mean_ink_mass"]/refs[polarity]["results"][kind][str(scale)]["chrome_ink"]["sizes"][size]["mean_ink_mass"])
                           for size,sample in current["sizes"].items()})
            results[kind][str(scale)]=record
    (out/"measurements.json").write_text(json.dumps(dict(stem_boost="compiled default; env override removed",results=results),indent=2))
    for kind,scales in results.items():
        for scale,d in scales.items():
            print(kind,scale,{p:(round(r["coverage"]["l1_relative_to_reference_ink_percent"],2),round(r["sizes"]["14"]["ink_ratio"],3)) for p,r in d.items()})


if __name__=="__main__":
    main()
