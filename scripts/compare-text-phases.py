#!/usr/bin/env python3
"""Controlled Latin/CJK hinting and physical-pixel phase experiment (optional Linux tools)."""
import argparse
import hashlib
import json
import os
import operator
from pathlib import Path
import runpy
import statistics
import subprocess
import sys

import cairo
from text_freetype import FreeType, composite_cairo

ROOT = Path(__file__).resolve().parent.parent
support = runpy.run_path(str(Path(__file__).with_name("compare-pango-text.py")))
VARIANTS = {"vello_hint": [], "vello_nohint": ["--no-hint"],
            "vello_snap_y": ["--snap", "y"], "vello_snap_xy": ["--snap", "xy"]}

for mode in ("none", "lcd", "light", "normal", "lcd-symmetric", "lcd-ft", "normal-ft", "light-ft"):
    VARIANTS[f"skrifa_{mode}"] = ["--outline-mode", mode]


def font(family):
    file, index, actual = subprocess.check_output([
        "fc-match", "-f", "%{file}\n%{index}\n%{family}", family
    ], text=True).strip().split("\n", 2)
    if family not in actual.split(","):
        raise RuntimeError(f"Install {family}; got {actual}")
    return file, int(index)


def fixture(family, file, text, scale):
    rows = []
    for size in (12, 14, 16):
        for dy in (0, .5):
            for dx in (0, .25, .5, .75):
                rows.append(dict(text=text, size=size, weight=400, x=20 + dx / scale,
                    baseline=34 + len(rows) * 48 + dy / scale,
                    phase_x=dx, phase_y=dy, sameGlyphs=True))
    return dict(width=660, height=len(rows) * 48 + 8, background="#000000", foreground="#ffffff",
                fontFamily=family, font_files=[file], scales=[scale], rows=rows)


def stats(path, case, scale):
    image = cairo.ImageSurface.create_from_png(str(path))
    data = image.get_data()
    channel = 0 if sys.byteorder == "little" else 1
    result = []
    for i, row in enumerate(case["rows"]):
        values, nonzero, soft = 0, 0, 0
        x0, y0, x1, y1 = image.get_width(), image.get_height(), -1, -1
        for y in range(round(i * 48 * scale), round((i + 1) * 48 * scale)):
            for x in range(image.get_width()):
                value = data[y * image.get_stride() + x * 4 + channel]
                values += value
                if value:
                    nonzero += 1
                    soft += value < 255
                    x0, y0, x1, y1 = min(x0, x), min(y0, y), max(x1, x), max(y1, y)
        assert nonzero, (path, i, "empty text")
        result.append(dict(size=row["size"], phase_x=row["phase_x"], phase_y=row["phase_y"],
            ink_mass=values / 255, soft_fraction=soft / nonzero,
            bbox=[x0, y0, x1 + 1, y1 + 1]))
    grouped = {}
    for size in (12, 14, 16):
        rows = [r for r in result if r["size"] == size]
        masses = [r["ink_mass"] for r in rows]
        grouped[str(size)] = dict(mean_ink_mass=statistics.mean(masses),
            mass_range_percent=(max(masses) - min(masses)) / statistics.mean(masses) * 100,
            mean_soft_fraction=statistics.mean(r["soft_fraction"] for r in rows))
    return dict(rows=result, sizes=grouped)


def coverage_difference(reference, candidate):
    a = cairo.ImageSurface.create_from_png(str(reference))
    b = cairo.ImageSurface.create_from_png(str(candidate))
    assert (a.get_width(), a.get_height()) == (b.get_width(), b.get_height())
    # White on black makes any RGB channel a directly observable coverage value.
    channel = 0 if sys.byteorder == "little" else 1
    av = bytes(a.get_data())[channel::4]
    bv = bytes(b.get_data())[channel::4]
    mass = sum(av)
    assert mass > 0
    error = sum(map(abs, map(operator.sub, av, bv)))
    return dict(l1_relative_to_reference_ink_percent=error / mass * 100,
                identical=av == bv)


def detail(files, scale, output):
    # 14px, y phase zero, x phases 0 and 0.5. Native pixels enlarged exactly 2x.
    width, height = round(470 * scale), round(48 * scale)
    row_h = height * 2 + 28
    canvas = cairo.ImageSurface(cairo.FORMAT_ARGB32, width * 4 + 48, row_h * len(files))
    ctx = cairo.Context(canvas)
    ctx.set_source_rgb(.85, .85, .85)
    ctx.paint()
    for i, (name, path) in enumerate(files.items()):
        src = cairo.ImageSurface.create_from_png(str(path))
        for col, index in enumerate((8, 10)):
            ctx.set_source_rgb(.05, .05, .05)
            ctx.set_font_size(16)
            ctx.move_to(12 + col * (width * 2 + 24), row_h * i + 20)
            ctx.show_text(f"{name} / x phase {col * .5:g}")
            ctx.save()
            ctx.rectangle(12 + col * (width * 2 + 24), row_h * i + 26, width * 2, height * 2)
            ctx.clip()
            ctx.translate(12 + col * (width * 2 + 24), row_h * i + 26)
            ctx.scale(2, 2)
            ctx.set_source_surface(src, 0, -round(index * 48 * scale))
            ctx.get_source().set_filter(cairo.FILTER_NEAREST)
            ctx.paint()
            ctx.restore()
    canvas.write_to_png(str(output))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "reports/hint-targets-exact")
    parser.add_argument("--scales", type=float, nargs='+', default=[1, 1.25, 1.5, 2])
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    ft = FreeType(out)
    results = {}
    env = {**os.environ, "PLICO_TEXT_STEM_BOOST": "0"}
    families = [("latin", "Noto Sans", "Hello Rust 0123456789 / The quick brown fox"),
                ("cjk", "Noto Sans CJK SC", "中文字体渲染：设置、连接、保存 0123")]
    metadata = dict(cairo=cairo.cairo_version_string(),
                    cargo_lock_sha256=hashlib.sha256((ROOT / "Cargo.lock").read_bytes()).hexdigest(),
                    foreground="white", background="black", stem_boost=0, fonts={})
    for kind, family, text in families:
        file, index = font(family)
        metadata["fonts"][kind] = dict(file=file, index=index,
            sha256=hashlib.sha256(Path(file).read_bytes()).hexdigest())
        metadata["freetype"] = ft.version(file)
        results[kind] = {}
        for scale in args.scales:
            if float(scale).is_integer():
                scale = int(scale)
            folder = out / kind / str(scale)
            folder.mkdir(parents=True, exist_ok=True)
            case = fixture(family, file, text, scale)
            manifest = folder / "case.json"
            manifest.write_text(json.dumps(case, ensure_ascii=False, indent=2))
            files = {}
            for name, flags in VARIANTS.items():
                dest = folder / name
                subprocess.run(["cargo", "run", "--locked", "--offline", "-p", "plico-shell",
                    "--example", "text_quality_probe", "--", "--output", str(dest),
                    "--cases", str(manifest), "--exact-baselines", *flags], cwd=ROOT, env=env, check=True)
                files[name] = dest / f"plico-text-quality-{scale}x.png"
            native = support["native_metrics"](folder / "vello_hint" / f"plico-text-quality-{scale}x.txt")
            assert len(native) == len(case["rows"])
            for row in native:
                assert row["fonts"] == [file] and row["indices"] == [index], row
            # Hint/position overrides occur only after layout. All shaping reports must match.
            # Font blob diagnostic IDs can vary across processes; verify actual glyph data instead.
            for name in VARIANTS:
                assert support["native_metrics"](folder / name / f"plico-text-quality-{scale}x.txt") == native
            physical = json.loads((folder / "vello_hint" / f"plico-physical-glyphs-{scale}x.json").read_text())
            assert len(physical) == sum(len(row["glyphs"]) for row in native)
            cursor = 0
            for row in native:
                count = len(row["glyphs"])
                row["physicalGlyphs"] = physical[cursor:cursor + count]
                assert [int(g[0]) for g in row["physicalGlyphs"]] == [int(g[0]) for g in row["glyphs"]]
                cursor += count
            for mode in ("none", "slight", "full"):

                mask_path = folder / f"ft-{mode}-mask.png"
                mask, _ = ft.mask(case, scale, native, mode, mask_path)
                path = folder / f"ft-{mode}-cairo.png"
                composite_cairo(mask, case, path)
                files[f"ft_{mode}"] = path
                if mode != "none":
                    aligned_path = folder / f"ft-{mode}-aligned-cairo.png"
                    aligned, _ = ft.mask(case, scale, native, mode, folder / f"ft-{mode}-aligned-mask.png", snap_y=True)
                    composite_cairo(aligned, case, aligned_path)
                    files[f"ft_{mode}_aligned"] = aligned_path
                # Vello composition was isolated in the preceding 24-case experiment.
            # Also verify the CJK mask through Vello, not only Cairo.
            vello_mask = folder / "ft-slight-vello.png"
            subprocess.run(["cargo", "run", "--locked", "--offline", "-p", "plico-shell",
                "--example", "glyph_mask_probe", "--", "--mask", str(folder / "ft-slight-mask.png"),
                "--output", str(vello_mask), "--foreground", case["foreground"],
                "--background", case["background"]], cwd=ROOT, env=env, check=True)
            files["ft_slight_vello"] = vello_mask
            measurements = {name: stats(path, case, scale) for name, path in files.items()}
            measurements["comparisons"] = {
                f"{reference}_vs_{candidate}": coverage_difference(files[reference], files[candidate])
                for reference, candidate in [("ft_none", "vello_nohint"), ("ft_none", "vello_hint"),
                    ("ft_slight", "vello_hint"), ("vello_hint", "skrifa_lcd"),
                    ("ft_none", "skrifa_none"), ("ft_full", "skrifa_normal-ft"),
                    ("ft_full_aligned", "skrifa_normal-ft"), ("ft_slight_aligned", "vello_hint"),
                    ("ft_slight", "skrifa_light-ft"), ("vello_hint", "skrifa_light"),
                    ("vello_hint", "skrifa_normal"), ("vello_hint", "skrifa_lcd-symmetric"),
                    ("skrifa_lcd-symmetric", "skrifa_lcd-ft"),
                    ("vello_hint", "vello_snap_y"),
                    ("vello_hint", "vello_snap_xy"), ("ft_slight", "ft_slight_vello")]
            }
            assert measurements["comparisons"]["vello_hint_vs_skrifa_lcd"]["identical"], "direct LCD outlines must reproduce production glyph path"
            assert measurements["comparisons"]["ft_slight_vs_ft_slight_vello"]["l1_relative_to_reference_ink_percent"] < 1.

            results[kind][str(scale)] = measurements
            detail(files, scale, out / f"{kind}-{scale}x.png")
    ft.close()
    (out / "measurements.json").write_text(json.dumps(dict(metadata=metadata, results=results), indent=2))
    lines = ["font,scale,size,variant,ink_mass_range_percent,soft_fraction"]
    for kind, scales in results.items():
        for scale, variants in scales.items():
            for variant, measurements in variants.items():
                if variant == "comparisons":
                    continue
                for size, sample in measurements["sizes"].items():
                    lines.append(f'{kind},{scale},{size},{variant},{sample["mass_range_percent"]:.5f},{sample["mean_soft_fraction"]:.5f}')
    (out / "phase-summary.csv").write_text("\n".join(lines) + "\n")
    print(f"Phase experiment: {out}")


if __name__ == "__main__":
    main()
