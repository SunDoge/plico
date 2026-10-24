#!/usr/bin/env python3
"""Compare the controlled glyph experiment with fresh headless Chrome Canvas text."""
import argparse
import html
import json
import os
import operator
import sys
from pathlib import Path
import re
import runpy
import shutil
import subprocess

import cairo

ROOT = Path(__file__).resolve().parent.parent


def page(case, scale, postscript, dom=False):
    return '''<!doctype html><html><head><meta charset="utf-8"><style>
html,body { margin:0; padding:0; overflow:hidden; background:black; }
canvas { display:block; }
</style></head><body data-state="loading"><canvas id="probe"></canvas><script>
(async () => {
 const c = CASE, scale = SCALE;
 const font = new FontFace("Probe", FONT_SOURCE);
 await font.load(); document.fonts.add(font); await document.fonts.ready;
 const canvas = document.getElementById("probe");
 canvas.width = Math.round(c.width*scale); canvas.height = Math.round(c.height*scale);
 canvas.style.width = c.width+"px"; canvas.style.height = c.height+"px";
 const ctx = canvas.getContext("2d", {alpha:false}); ctx.scale(scale,scale);
 ctx.fillStyle=c.background; ctx.fillRect(0,0,c.width,c.height);
 ctx.fillStyle=c.foreground; ctx.textBaseline="alphabetic"; ctx.fontKerning="normal";
 const widths=[];
 for (const row of c.rows) {
   ctx.font=`${row.weight} ${row.size}px Probe`;
   widths.push(ctx.measureText(row.text).width);
   ctx.fillText(row.text,row.x,row.baseline);
 }
 const baselineErrors=[];
 if (USE_DOM) {
   canvas.style.display="none"; document.body.style.width=c.width+"px";
   document.body.style.height=c.height+"px";document.body.style.background=c.background;
   c.rows.forEach((row,index) => {
     const div=document.createElement("div");
     Object.assign(div.style,{position:"absolute",left:row.x+"px",top:"0px",
       whiteSpace:"pre",font:`${row.weight} ${row.size}px Probe`,lineHeight:"1",color:c.foreground});
     const span=document.createElement("span");span.textContent=row.text;
     const marker=document.createElement("i");
     Object.assign(marker.style,{display:"inline-block",width:"0px",height:"0px",verticalAlign:"baseline"});
     div.append(span,marker);document.body.append(div);
     const baseline=marker.getBoundingClientRect().top;
     div.style.top=(row.baseline-baseline)+"px";
     baselineErrors.push(marker.getBoundingClientRect().top-row.baseline);
     widths[index]=span.getBoundingClientRect().width;
   });
 }
 document.body.dataset.metrics=JSON.stringify({dpr:devicePixelRatio,widths,baselineErrors});
 document.body.dataset.state="ready";
})().catch(error => {document.body.dataset.state=String(error)});
</script></body></html>'''.replace("CASE", json.dumps(case, ensure_ascii=False)).replace("SCALE", str(scale)).replace(
    "FONT_SOURCE", json.dumps(f'local("{postscript}")')).replace("USE_DOM", str(dom).lower())


def invert_rgb(source, destination):
    image=cairo.ImageSurface.create_from_png(str(source))
    original=bytes(image.get_data()); data=bytearray(original)
    channels=(0,1,2) if sys.byteorder=="little" else (1,2,3)
    table=bytes(255-i for i in range(256))
    for channel in channels: data[channel::4]=original[channel::4].translate(table)
    out=cairo.ImageSurface.create_for_data(data,cairo.FORMAT_ARGB32,image.get_width(),image.get_height(),image.get_stride())
    out.write_to_png(str(destination))


def aligned_rows(reference, candidate, case, scale):
    a, b = [cairo.ImageSurface.create_from_png(str(path)) for path in (reference, candidate)]
    channel = 0 if sys.byteorder == "little" else 1
    av, bv = bytes(a.get_data()), bytes(b.get_data())
    width = a.get_width()
    records = []
    total_raw = total_best = total_mass = 0
    for i, row in enumerate(case["rows"]):
        y0, y1 = round(i * 48 * scale), round((i + 1) * 48 * scale)
        left = [av[y*a.get_stride()+channel:y*a.get_stride()+width*4:4] for y in range(y0,y1)]
        right = [bv[y*b.get_stride()+channel:y*b.get_stride()+width*4:4] for y in range(y0,y1)]
        ref = b"".join(left)
        mass = sum(ref)
        raw = sum(map(abs,map(operator.sub,ref,b"".join(right))))
        best = (raw,0,0)
        for dy in (-1,0,1):
            for dx in (-1,0,1):
                shifted = []
                for y in range(len(right)):
                    line = right[y-dy] if 0 <= y-dy < len(right) else bytes(width)
                    line = (bytes(dx)+line[:width-dx]) if dx>0 else (line[-dx:]+bytes(-dx)) if dx<0 else line
                    shifted.append(line)
                error = sum(map(abs,map(operator.sub,ref,b"".join(shifted))))
                if error < best[0]: best=(error,dx,dy)
        total_raw+=raw; total_best+=best[0]; total_mass+=mass
        records.append(dict(size=row["size"],phase_x=row["phase_x"],phase_y=row["phase_y"],
            raw_l1_percent=raw/mass*100,aligned_l1_percent=best[0]/mass*100,dx=best[1],dy=best[2]))
    return dict(raw_l1_percent=total_raw/total_mass*100,aligned_l1_percent=total_best/total_mass*100,
        shifted_rows=sum(r["dx"]!=0 or r["dy"]!=0 for r in records),rows=records)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=ROOT / "reports/hint-targets-exact")
    parser.add_argument("--output", type=Path, default=ROOT / "reports/chrome-text-exact")
    parser.add_argument("--chrome", default=shutil.which("google-chrome"))
    parser.add_argument("--scales", type=float, nargs='+', default=[1, 1.25, 1.5, 2])
    parser.add_argument("--dom", action="store_true", help="Use CSS text nodes instead of Canvas glyphs")
    parser.add_argument("--light", action="store_true", help="Black text on white; validate actual native inversion")
    args = parser.parse_args()
    if not args.chrome:
        parser.error("Chrome not found; pass --chrome")
    source, out = args.source.resolve(), args.output.resolve()
    if args.dom:
        out=out.with_name(out.name+"-dom")
    if args.light:
        out=out.with_name(out.name+"-light")
    out.mkdir(parents=True, exist_ok=True)
    tools = runpy.run_path(str(Path(__file__).with_name("compare-text-phases.py")))
    results = {}
    version = subprocess.check_output([args.chrome, "--version"], text=True).strip()
    variants = ("vello_hint", "vello_nohint", "skrifa_lcd-symmetric", "skrifa_normal-ft",
                "ft_slight_aligned", "ft_full_aligned")
    for kind in ("latin", "cjk"):
        results[kind] = {}
        for scale in args.scales:
            if float(scale).is_integer():
                scale = int(scale)
            folder = source / kind / str(scale)
            case = json.loads((folder / "case.json").read_text())
            if args.light:
                case["foreground"],case["background"]="#000000","#ffffff"
            postscript = subprocess.check_output(["fc-match", "-f", "%{postscriptname}", case["fontFamily"]], text=True).strip()
            target = out / kind / str(scale)
            target.mkdir(parents=True, exist_ok=True)
            path = target / "case.html"
            path.write_text(page(case, scale, postscript, args.dom))
            screenshot = target / "chrome.png"
            cmd = [args.chrome, "--headless=new", "--disable-background-networking", "--no-first-run",
                "--no-default-browser-check", "--hide-scrollbars", "--virtual-time-budget=2000",
                f'--window-size={case["width"]},{case["height"]}', f"--force-device-scale-factor={scale}",
                f"--user-data-dir={target / 'profile'}", f"--screenshot={screenshot}", "--dump-dom", path.as_uri()]
            if os.geteuid() == 0:
                cmd.insert(1, "--no-sandbox")
            proc = subprocess.run(cmd, capture_output=True, text=True, timeout=45, check=True)
            (target / "chrome.log").write_text(proc.stderr)
            assert 'data-state="ready"' in proc.stdout, proc.stdout[-2000:]
            metrics = json.loads(html.unescape(re.search(r'data-metrics="([^"]+)"', proc.stdout)[1]))
            assert metrics["dpr"] == scale
            image = cairo.ImageSurface.create_from_png(str(screenshot))
            assert (image.get_width(), image.get_height()) == (round(case["width"]*scale), round(case["height"]*scale))
            native = tools["support"]["native_metrics"](folder / "vello_hint" / f"plico-text-quality-{scale}x.txt")
            widths = [b-a["advance"] for a,b in zip(native,metrics["widths"])]
            files = {"chrome_canvas": screenshot}
            for variant in variants:
                if variant.startswith("ft_"):
                    mode = "slight" if "slight" in variant else "full"
                    files[variant] = folder / f"ft-{mode}-aligned-cairo.png"
                else:
                    files[variant] = folder / variant / f"plico-text-quality-{scale}x.png"
            coverage_files=dict(files)
            if args.light:
                coverage_screenshot=target/"chrome-coverage.png"
                invert_rgb(screenshot,coverage_screenshot)
                coverage_files["chrome_canvas"]=coverage_screenshot
                # Coverage is polarity-independent for these native outline commands;
                # verify the analytical inversion against an actual native light render.
                native_case=target/"native-case.json"
                native_case.write_text(json.dumps(case,ensure_ascii=False))
                subprocess.run(["cargo","run","--locked","--offline","-p","plico-shell",
                    "--example","text_quality_probe","--","--cases",str(native_case),
                    "--output",str(target/"native"),"--exact-baselines"],cwd=ROOT,
                    env={**os.environ,"PLICO_TEXT_STEM_BOOST":"0"},check=True,
                    stdout=subprocess.DEVNULL)
                inverted_native=target/"native-inverted.png"
                native_light=target/"native"/f"plico-text-quality-{scale}x.png"
                invert_rgb(native_light,inverted_native)
                assert tools["coverage_difference"](files["vello_hint"],inverted_native)["identical"], "native polarity must invert exactly"
                for name,file in list(files.items()):
                    if name!="chrome_canvas":
                        light_file=target/f"{name}-light.png"
                        invert_rgb(file,light_file)
                        files[name]=light_file
            coverage_reference=coverage_files["chrome_canvas"]
            comparisons = {name:tools["coverage_difference"](coverage_reference,file) for name,file in coverage_files.items() if name!="chrome_canvas"}
            alignment = {name:aligned_rows(coverage_reference,file,case,scale) for name,file in coverage_files.items() if name!="chrome_canvas"}
            chrome_ink = tools["stats"](coverage_reference,case,scale)
            tools["detail"](files,scale,out / f"{kind}-{scale}x.png")
            results[kind][str(scale)] = dict(postscript=postscript, width_deltas=widths,
                max_abs_width_delta=max(map(abs,widths)),baseline_errors=metrics["baselineErrors"], comparisons=comparisons, alignment=alignment, chrome_ink=chrome_ink)
    (out / "measurements.json").write_text(json.dumps(dict(chrome=version,light=args.light,dom=args.dom,results=results),indent=2))
    print(f"Chrome comparison: {out}")


if __name__ == "__main__":
    main()
