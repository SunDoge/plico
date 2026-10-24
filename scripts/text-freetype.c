/* Optional comparison shim. System FreeType headers define all ABI layouts. */
#include <ft2build.h>
#include FT_FREETYPE_H
#include <stdlib.h>

typedef struct { FT_Library library; FT_Face face; } ProbeFace;
typedef struct {
    int width, rows, pitch, left, top;
    const unsigned char *data;
} ProbeBitmap;

void *probe_face_new(const char *path, long face_index) {
    ProbeFace *p = calloc(1, sizeof(*p));
    if (!p) return NULL;
    if (FT_Init_FreeType(&p->library)) { free(p); return NULL; }
    if (FT_New_Face(p->library, path, face_index, &p->face)) {
        FT_Done_FreeType(p->library); free(p); return NULL;
    }
    return p;
}
void probe_face_free(void *handle) {
    ProbeFace *p = handle;
    if (!p) return;
    FT_Done_Face(p->face); FT_Done_FreeType(p->library); free(p);
}
void probe_version(void *handle, int *major, int *minor, int *patch) {
    ProbeFace *p = handle;
    FT_Library_Version(p->library, major, minor, patch);
}
/* Returned bytes are borrowed until the next probe_glyph call or face_free. */
int probe_glyph(void *handle, unsigned int glyph, long size_26_6,
                long phase_x, long phase_y, int hint, ProbeBitmap *out) {
    ProbeFace *p = handle;
    int error = FT_Set_Char_Size(p->face, 0, size_26_6, 72, 72);
    if (error) return error;
    FT_Vector delta = { phase_x, -phase_y }; /* screen Y points down */
    FT_Set_Transform(p->face, NULL, &delta);
    FT_Int32 flags = FT_LOAD_NO_BITMAP;
    if (hint == 0) flags |= FT_LOAD_NO_HINTING | FT_LOAD_NO_AUTOHINT;
    else if (hint == 1) flags |= FT_LOAD_TARGET_LIGHT;
    else flags |= FT_LOAD_TARGET_NORMAL;
    error = FT_Load_Glyph(p->face, glyph, flags);
    if (error) return error;
    error = FT_Render_Glyph(p->face->glyph, FT_RENDER_MODE_NORMAL);
    if (error) return error;
    FT_GlyphSlot g = p->face->glyph;
    if (g->bitmap.width && g->bitmap.rows && g->bitmap.pixel_mode != FT_PIXEL_MODE_GRAY) return -1;
    if (g->bitmap.width > 4096 || g->bitmap.rows > 4096) return -2;
    *out = (ProbeBitmap){g->bitmap.width, g->bitmap.rows, g->bitmap.pitch,
                        g->bitmap_left, g->bitmap_top, g->bitmap.buffer};
    return 0;
}
