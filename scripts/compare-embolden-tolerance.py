#!/usr/bin/env python3
"""Isolate outline expansion tolerance while holding hint, amount and positions fixed."""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import runpy
import subprocess

ROOT = Path(__file__).resolve().parent.parent
red = runpy.run_path(str(ROOT/'scripts/compare-text-colors.py'))['red']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,default=ROOT/'reports/embolden-tolerance')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True,exist_ok=True)
    results = []
    for font in ('latin','cjk'):
        source = ROOT/'reports/hint-targets-175'/font/'1.75'
        baseline = None
        reference = red(ROOT/'reports/chrome-text-175'/font/'1.75/chrome.png')
        mass = sum(reference)
        for tolerance in (.1,.01,.001):
            target = out/font/str(tolerance)
            subprocess.run(['cargo','run','--locked','--offline','-p','plico-shell',
                '--example','text_quality_probe','--','--cases',str(source/'case.json'),
                '--output',str(target),'--exact-baselines','--outline-mode','lcd',
                '--embolden-tolerance',str(tolerance)],cwd=ROOT,check=True,
                env={**os.environ,'PLICO_TEXT_STEM_BOOST':'2'},stdout=subprocess.DEVNULL)
            candidate = red(target/'plico-text-quality-1.75x.png')
            assert len(candidate)==len(reference)
            if baseline is None:
                baseline = candidate  # Historical coarse expansion, not the repaired default.
            counts = Counter(candidate)
            record = dict(font=font,tolerance=tolerance,
                changed_pixels_from_coarse=sum(a!=b for a,b in zip(baseline,candidate)),
                l1_to_chrome_percent=sum(abs(a-b) for a,b in zip(reference,candidate))/mass*100,
                ink_ratio=sum(candidate)/mass,
                faint_pixels=sum(counts[v] for v in range(1,16)))
            results.append(record)
            print(record,flush=True)
    (out/'measurements.json').write_text(json.dumps(results,indent=2))


if __name__=='__main__':
    main()
