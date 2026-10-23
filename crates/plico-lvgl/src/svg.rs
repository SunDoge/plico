//! Mounted SVGs retain a raster until their source, size or resolved color changes.
//! LVGL owns the pixels; the descriptor stays pinned while the native image uses it.
use super::{Error, SvgRasterSnafu};
use lightvgl_sys as sys;
use plico_core::{NodeRef, Theme};
use snafu::ensure;

pub(super) struct Raster {
    source: String,
    color: u32,
    width: u32,
    height: u32,
    image: Box<sys::lv_image_dsc_t>,
    buffer: *mut sys::lv_draw_buf_t,
}
impl Raster {
    pub(super) fn matches(&self, node: &NodeRef<'_>, theme: Theme) -> bool {
        let (width, height) = dimensions(node);
        self.source == node.text
            && self.color == theme.resolve(node.style.foreground)
            && self.width == width
            && self.height == height
    }
    pub(super) fn bytes(&self) -> usize {
        unsafe { (*self.buffer).data_size as usize }
    }
    pub(super) fn image(&self) -> *const sys::lv_image_dsc_t {
        &*self.image
    }
    pub(super) fn new(node: &NodeRef<'_>, theme: Theme) -> Result<Self, Error> {
        let (width, height) = dimensions(node);
        // Bound a raster's allocation; percent sizing is not supported yet.
        ensure!(
            (1..=2048).contains(&width) && (1..=2048).contains(&height),
            SvgRasterSnafu
        );
        let color = theme.resolve(node.style.foreground);
        let markup = node.text.replace("currentColor", &format!("#{color:06x}"));
        unsafe {
            let mut source: sys::lv_image_dsc_t = std::mem::zeroed();
            source.header.set_magic(sys::LV_IMAGE_HEADER_MAGIC);
            source.header.set_w(width);
            source.header.set_h(height);
            source.data_size = markup.len() as u32;
            source.data = markup.as_ptr();
            let root = sys::lv_obj_create(std::ptr::null_mut());
            ensure!(!root.is_null(), SvgRasterSnafu);
            // A screen always takes the display size, even after set_size.
            // Snapshot a child container so a 24px icon allocates only 24px.
            let canvas = sys::lv_obj_create(root);
            if canvas.is_null() {
                sys::lv_obj_delete(root);
                return SvgRasterSnafu.fail();
            }
            sys::lv_obj_remove_style_all(canvas);
            sys::lv_obj_set_size(canvas, width as i32, height as i32);
            let image_obj = sys::lv_image_create(canvas);
            if image_obj.is_null() {
                sys::lv_obj_delete(root);
                return SvgRasterSnafu.fail();
            }
            sys::lv_image_set_src(image_obj, &source as *const _ as *const _);
            let buffer =
                sys::lv_snapshot_take(canvas, sys::lv_color_format_t_LV_COLOR_FORMAT_ARGB8888);
            sys::lv_obj_delete(root);
            sys::lv_image_cache_drop(&source as *const _ as *const _);
            ensure!(!buffer.is_null(), SvgRasterSnafu);
            let mut image = Box::new(std::mem::zeroed());
            sys::lv_draw_buf_to_image(buffer, &mut *image);
            Ok(Self {
                source: node.text.into(),
                color,
                width,
                height,
                image,
                buffer,
            })
        }
    }
}
impl Drop for Raster {
    fn drop(&mut self) {
        unsafe {
            sys::lv_image_cache_drop(self.image() as *const _);
            sys::lv_draw_buf_destroy(self.buffer);
        }
    }
}
fn dimensions(node: &NodeRef<'_>) -> (u32, u32) {
    (
        node.style.width.unwrap_or(24.).max(0.) as u32,
        node.style.height.unwrap_or(24.).max(0.) as u32,
    )
}
