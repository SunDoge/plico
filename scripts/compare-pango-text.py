#!/usr/bin/env python3
"""Linux-only, optional text raster/layout comparison. Never changes the backend."""
import argparse
import hashlib
import sys
import json
import os
from pathlib import Path
import platform
import re
from text_freetype import FreeType, composite_cairo
import subprocess

import cairo
import gi

gi.require_foreign("cairo")
gi.require_version("Pango", "1.0")
gi.require_version("PangoCairo", "1.0")
from gi.repository import Pango, PangoCairo  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
MODES = {"none": (cairo.HINT_STYLE_NONE, False),
         "slight": (cairo.HINT_STYLE_SLIGHT, False),
         "full": (cairo.HINT_STYLE_FULL, True)}


def rgb(value):
    return tuple(int(value[i:i + 2], 16) / 255 for i in (1, 3, 5))


def font_file(weight):
    family, file = subprocess.check_output([
        "fc-match", "-f", "%{family}\n%{file}", f"Noto Sans:weight={weight}"
    ], text=True).strip().split("\n", 1)
    if "Noto Sans" not in family.split(","):
        raise RuntimeError(f"Install Noto Sans; Fontconfig substituted {family}")
    return file


def fixture(background, foreground):
    rows = []
    texts = ["Hello Rust 0123456789 / The quick brown fox",
             "中文字体渲染：设置、连接、保存。",
             "Hello العربية مرحبا Rust", "Emoji: 🇬🇧 🇯🇵 👩🏽‍💻 ❤️"]
    for size in (12, 13, 14, 16):
        for index, text in enumerate(texts):
            rows.append(dict(text=text, size=size, weight=400, x=20,
                             baseline=34 + len(rows) * 40, sameGlyphs=index == 0))
    rows.append(dict(text=texts[0], size=14, weight=700, x=20,
                     baseline=34 + len(rows) * 40, sameGlyphs=True))
    return dict(width=760, height=700, background=background, foreground=foreground,
                fontFamily="Noto Sans", font_files=[font_file("regular"), font_file("bold")],
                scales=[1, 1.25, 1.5, 2], rows=rows)


def surface(case, scale):
    image = cairo.ImageSurface(cairo.FORMAT_ARGB32,
                               round(case["width"] * scale), round(case["height"] * scale))
    ctx = cairo.Context(image)
    ctx.scale(scale, scale)
    ctx.set_source_rgb(*rgb(case["background"]))
    ctx.paint()
    ctx.set_source_rgb(*rgb(case["foreground"]))
    return image, ctx


def options(mode):
    hint, metrics = MODES[mode]
    opts = cairo.FontOptions()
    opts.set_antialias(cairo.ANTIALIAS_GRAY)
    opts.set_hint_style(hint)
    opts.set_hint_metrics(cairo.HINT_METRICS_ON if metrics else cairo.HINT_METRICS_OFF)
    return opts


def pango(case, scale, mode, path):
    image, ctx = surface(case, scale)
    ctx.set_font_options(options(mode))
    layout = PangoCairo.create_layout(ctx)
    context = layout.get_context()
    PangoCairo.context_set_font_options(context, options(mode))
    context.set_round_glyph_positions(MODES[mode][1])
    metrics = []
    for row in case["rows"]:
        desc = Pango.FontDescription.from_string(case["fontFamily"])
        desc.set_absolute_size(row["size"] * Pango.SCALE)
        desc.set_weight(row["weight"])
        layout.set_font_description(desc)
        layout.set_text(row["text"], -1)
        baseline = layout.get_baseline() / Pango.SCALE
        ink, logical = layout.get_extents()
        ctx.move_to(row["x"], row["baseline"] - baseline)
        PangoCairo.show_layout(ctx, layout)
        metrics.append(dict(advance=logical.width / Pango.SCALE, baseline=baseline,
                            ink=[ink.x / Pango.SCALE, ink.y / Pango.SCALE,
                                 ink.width / Pango.SCALE, ink.height / Pango.SCALE]))
    image.write_to_png(str(path))
    return metrics


def native_metrics(path):
    rows = []
    for line in path.read_text().splitlines():
        if line.startswith("row="):
            fields = dict(re.findall(r"(\w+)=([^ ]+)", line))
            rows.append(dict(advance=float(fields["advance"]), baseline=float(fields["baseline"]),
                             glyphs=[], fonts=[], indices=[]))
        elif line.startswith("  font_file="):
            rows[-1]["fonts"].append(line.split("=", 1)[1])
        elif " glyphs=" in line:
            rows[-1]["indices"].append(int(re.search(r"font_index=(\d+)", line)[1]))
            rows[-1]["glyphs"].extend(tuple(float(x) for x in glyph.split(":"))
                                        for glyph in line.split(" glyphs=", 1)[1].split(",") if glyph)
    return rows


def same_glyphs(case, scale, native, path):
    """Replay Parley glyph IDs/positions through Cairo, restricted to verified ASCII runs."""
    image, ctx = surface(case, scale)
    ctx.set_font_options(options("slight"))
    verified = []
    for index, (row, metrics) in enumerate(zip(case["rows"], native)):
        if not row["sameGlyphs"]:
            continue
        assert len(metrics["fonts"]) == 1 and metrics["fonts"][0] in case["font_files"], metrics
        ctx.select_font_face(case["fontFamily"], cairo.FONT_SLANT_NORMAL,
                             cairo.FONT_WEIGHT_BOLD if row["weight"] == 700 else cairo.FONT_WEIGHT_NORMAL)
        ctx.set_font_size(row["size"])
        # Verify Cairo's independently selected face agrees on every ASCII glyph ID.
        # Exact file identity for Cairo's toy API is not exposed; record this limit.
        cairo_ids = [g[0] for g in ctx.get_scaled_font().text_to_glyphs(0, 0, row["text"], False)]
        parley_ids = [int(g[0]) for g in metrics["glyphs"]]
        assert cairo_ids == parley_ids, (index, "font/glyph mapping differs")
        ctx.show_glyphs([(int(gid), row["x"] + x, row["baseline"] - metrics["baseline"] + y)
                         for gid, x, y in metrics["glyphs"]])
        verified.append(index)
    image.write_to_png(str(path))
    return verified


def detail(case, scale, files, output):
    # Native pixels enlarged 2x with nearest-neighbour filtering, no resampling blur.
    crop_w, crop_h = round(520 * scale), round(40 * scale)
    row_h = crop_h * 2 + 30
    canvas = cairo.ImageSurface(cairo.FORMAT_ARGB32, crop_w * 2 + 32, row_h * len(files))
    ctx = cairo.Context(canvas)
    ctx.set_source_rgb(.88, .88, .88)
    ctx.paint()
    for i, (name, path) in enumerate(files):
        ctx.set_source_rgb(.08, .08, .08)
        ctx.select_font_face("sans")
        ctx.set_font_size(16)
        ctx.move_to(16, row_h * i + 21)
        ctx.show_text(name)
        source = cairo.ImageSurface.create_from_png(str(path))
        ctx.save()
        ctx.rectangle(16, row_h * i + 28, crop_w * 2, crop_h * 2)
        ctx.clip()
        ctx.translate(16, row_h * i + 28)
        ctx.scale(2, 2)
        # Row 8 is 14px Latin; remove the preceding line and align all baselines.
        ctx.set_source_surface(source, 0, -round(8 * 40 * scale))
        ctx.get_source().set_filter(cairo.FILTER_NEAREST)
        ctx.paint()
        ctx.restore()
    canvas.write_to_png(str(output))


def raster_difference(reference, candidate, case, scale):
    a = cairo.ImageSurface.create_from_png(str(reference))
    b = cairo.ImageSurface.create_from_png(str(candidate))
    assert (a.get_width(), a.get_height()) == (b.get_width(), b.get_height())
    av, bv = a.get_data(), b.get_data()
    channels = (0, 1, 2) if sys.byteorder == "little" else (1, 2, 3)
    delta, changed, samples, peak = 0, 0, 0, 0
    # Identical 14px Latin row, including whitespace. Difference is not a quality score.
    for y in range(round(320 * scale), round(360 * scale)):
        for x in range(round(20 * scale), round(340 * scale)):
            differences = [abs(av[y * a.get_stride() + x * 4 + c] -
                               bv[y * b.get_stride() + x * 4 + c]) for c in channels]
            peak = max(peak, *differences)
            pixel_delta = sum(differences)
            delta += pixel_delta
            changed += pixel_delta != 0
            samples += 1
    return dict(mean_absolute_rgb_error_255=delta / (samples * 3),
                max_absolute_rgb_error_255=peak,
                changed_pixel_fraction=changed / samples,
                roi_logical=[20, 320, 340, 360])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "reports/pango-text")
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    freetype = FreeType(out)
    metadata = dict(platform=platform.platform(), cairo=cairo.cairo_version_string(),
                    pango=Pango.version_string(), modes={k: dict(hinting=k, gray=True,
                    rounded_positions=v[1], hint_metrics=v[1]) for k, v in MODES.items()},
                    font_files=[font_file("regular"), font_file("bold")])
    metadata["font_sha256"] = {path: hashlib.sha256(Path(path).read_bytes()).hexdigest() for path in metadata["font_files"]}
    metadata["cargo_lock_sha256"] = hashlib.sha256((ROOT / "Cargo.lock").read_bytes()).hexdigest()
    metadata["freetype"] = freetype.version(metadata["font_files"][0])
    metadata["mask_phase_quantization"] = "1/64 physical pixel; no embedded bitmaps or synthetic embolden"
    results = {}
    for theme, bg, fg in [("light", "#f4f4f5", "#202024"), ("dark", "#16181d", "#e4e7ed")]:
        case = fixture(bg, fg)
        manifest = out / f"{theme}.json"
        manifest.write_text(json.dumps(case, ensure_ascii=False, indent=2))
        for boost in (0, 2):
            destination = out / theme / f"plico-boost{boost}"
            destination.mkdir(parents=True, exist_ok=True)
            subprocess.run(["cargo", "run", "--locked", "--offline", "-p", "plico-shell", "--example",
                            "text_quality_probe", "--", "--output", str(destination),
                            "--cases", str(manifest)], cwd=ROOT,
                           env={**os.environ, "PLICO_TEXT_STEM_BOOST": str(boost)}, check=True)
        results[theme] = {}
        for scale in case["scales"]:
            native = native_metrics(out / theme / "plico-boost0" / f"plico-text-quality-{scale}x.txt")
            assert len(native) == len(case["rows"])
            result = dict(parley=native, pango={})
            files = [(f"Plico / Parley -> Vello / boost={boost} / {scale}x", out / theme /
                      f"plico-boost{boost}" / f"plico-text-quality-{scale}x.png") for boost in (0, 2)]
            for mode in MODES:
                path = out / theme / f"pango-{mode}-{scale}x.png"
                measurements = pango(case, scale, mode, path)
                for row, reference in zip(measurements, native):
                    row["advance_delta_from_parley"] = row["advance"] - reference["advance"]
                result["pango"][mode] = measurements
                files.append((f"Pango -> Cairo / gray / hint={mode} / {scale}x", path))
            path = out / theme / f"parley-cairo-{scale}x.png"
            result["same_glyph_verified_rows"] = same_glyphs(case, scale, native, path)
            files.append((f"Same Parley glyphs -> Cairo / slight / no stem boost / {scale}x", path))
            result["freetype"] = {}
            for mode in MODES:
                mask_path = out / theme / f"freetype-{mode}-{scale}x-mask.png"
                mask, count = freetype.mask(case, scale, native, mode, mask_path)
                cairo_path = out / theme / f"freetype-{mode}-cairo-{scale}x.png"
                vello_path = out / theme / f"freetype-{mode}-vello-{scale}x.png"
                composite_cairo(mask, case, cairo_path)
                subprocess.run(["cargo", "run", "--locked", "--offline", "-p", "plico-shell",
                    "--example", "glyph_mask_probe", "--", "--mask", str(mask_path),
                    "--output", str(vello_path), "--foreground", case["foreground"],
                    "--background", case["background"]], cwd=ROOT, check=True)
                difference = raster_difference(cairo_path, vello_path, case, scale)
                result["freetype"][mode] = dict(glyph_count=count, cairo_vs_vello=difference)
                # An exact mask and 1:1 placement must not develop material blending errors.
                assert difference["mean_absolute_rgb_error_255"] < 1.0, difference
                if mode == "slight":
                    files.extend([(f"Parley -> FreeType LIGHT -> Cairo / {scale}x", cairo_path),
                                  (f"Same FreeType mask -> Vello / {scale}x", vello_path)])
            native_path = files[0][1]
            result["raster_difference_from_plico_boost0"] = {
                name: raster_difference(native_path, file, case, scale) for name, file in files[1:]
            }
            detail(case, scale, files, out / f"{theme}-detail-{scale}x.png")
            results[theme][str(scale)] = result
    freetype.close()
    (out / "measurements.json").write_text(json.dumps(dict(metadata=metadata, results=results),
                                                     ensure_ascii=False, indent=2))
    print(f"Comparison images and measurements: {out}")


if __name__ == "__main__":
    main()
