"""Optional experiment support; direct FreeType rasterization of supplied glyph IDs."""
import ctypes as C
import math
from pathlib import Path
import shlex
import subprocess
import cairo


class Bitmap(C.Structure):
    _fields_ = [(name, C.c_int) for name in ("width", "rows", "pitch", "left", "top")] + [("data", C.c_void_p)]


class FreeType:
    def __init__(self, output):
        source = Path(__file__).with_name("text-freetype.c")
        library = output / "libtext-freetype.so"
        flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs", "freetype2"], text=True))
        subprocess.run(["cc", "-shared", "-fPIC", "-O2", "-Wall", "-Wextra", "-Werror",
                        str(source), "-o", str(library), *flags], check=True)
        self.lib = C.CDLL(str(library))
        self.lib.probe_face_new.argtypes = [C.c_char_p, C.c_long]
        self.lib.probe_face_new.restype = C.c_void_p
        self.lib.probe_face_free.argtypes = [C.c_void_p]
        self.lib.probe_face_free.restype = None
        self.lib.probe_glyph.argtypes = [C.c_void_p, C.c_uint, C.c_long, C.c_long,
                                        C.c_long, C.c_int, C.POINTER(Bitmap)]
        self.lib.probe_glyph.restype = C.c_int
        self.lib.probe_version.argtypes = [C.c_void_p] + [C.POINTER(C.c_int)] * 3
        self.lib.probe_version.restype = None
        self.faces = {}

    def face(self, path, index=0):
        key = (path, index)
        if key not in self.faces:
            face = self.lib.probe_face_new(str(path).encode(), index)
            if not face:
                raise RuntimeError(f"FreeType could not load {path}")
            self.faces[key] = face
        return self.faces[key]

    def version(self, path):
        parts = [C.c_int() for _ in range(3)]
        self.lib.probe_version(self.face(path), *[C.byref(part) for part in parts])
        return ".".join(str(part.value) for part in parts)

    def close(self):
        for face in self.faces.values():
            self.lib.probe_face_free(face)
        self.faces.clear()

    def mask(self, case, scale, native, mode, path, snap_y=False, x_steps=None):
        width, height = round(case["width"] * scale), round(case["height"] * scale)
        mask = cairo.ImageSurface(cairo.FORMAT_A8, width, height)
        ctx = cairo.Context(mask)
        ctx.set_source_rgba(1, 1, 1, 1)
        glyph_count = 0
        for row, metrics in zip(case["rows"], native):
            if not row["sameGlyphs"]:
                continue
            assert len(metrics["fonts"]) == 1 and metrics["fonts"][0] in case["font_files"]
            assert len(metrics["indices"]) == 1, "one face per controlled row"
            face = self.face(metrics["fonts"][0], metrics["indices"][0])
            positions = metrics.get("physicalGlyphs")
            if positions is None:
                positions = [(glyph, (row["x"] + x) * scale,
                              (row["baseline"] - metrics["baseline"] + y) * scale)
                             for glyph, x, y in metrics["glyphs"]]
            for glyph, x, y in positions:
                if x_steps is not None:
                    x = math.floor(x * x_steps + 0.5) / x_steps
                if snap_y:
                    y = math.floor(y + 0.5)
                px, py = math.floor(x), math.floor(y)
                bitmap = Bitmap()
                error = self.lib.probe_glyph(face, int(glyph), round(row["size"] * scale * 64),
                    round((x - px) * 64), round((y - py) * 64),
                    {"none": 0, "slight": 1, "full": 2}[mode], C.byref(bitmap))
                if error:
                    raise RuntimeError(f"FreeType glyph {glyph} failed: {error}")
                glyph_count += 1
                if not bitmap.width or not bitmap.rows:
                    continue
                assert bitmap.pitch >= bitmap.width, "unsupported negative pitch"
                stride = cairo.ImageSurface.format_stride_for_width(cairo.FORMAT_A8, bitmap.width)
                pixels = bytearray(stride * bitmap.rows)
                for line in range(bitmap.rows):
                    pixels[line * stride:line * stride + bitmap.width] = C.string_at(
                        bitmap.data + line * bitmap.pitch, bitmap.width)
                tile = cairo.ImageSurface.create_for_data(pixels, cairo.FORMAT_A8,
                                                           bitmap.width, bitmap.rows, stride)
                # Integer placement: subpixel phase was already applied by FreeType.
                ctx.mask_surface(tile, px + bitmap.left, py - bitmap.top)
        # Cairo writes A8 PNGs as grayscale intensity, not PNG alpha. Export
        # explicit white RGBA coverage so the consumer cannot confuse the two.
        rgba = cairo.ImageSurface(cairo.FORMAT_ARGB32, width, height)
        export = cairo.Context(rgba)
        export.set_source_rgba(1, 1, 1, 1)
        export.mask_surface(mask, 0, 0)
        rgba.write_to_png(str(path))
        return mask, glyph_count


def composite_cairo(mask, case, path):
    image = cairo.ImageSurface(cairo.FORMAT_ARGB32, mask.get_width(), mask.get_height())
    ctx = cairo.Context(image)
    def rgb(value):
        return tuple(int(value[i:i + 2], 16) / 255 for i in (1, 3, 5))
    ctx.set_source_rgb(*rgb(case["background"]))
    ctx.paint()
    ctx.set_source_rgb(*rgb(case["foreground"]))
    ctx.mask_surface(mask, 0, 0)
    image.write_to_png(str(path))
