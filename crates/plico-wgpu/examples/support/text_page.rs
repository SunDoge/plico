use plico_wgpu::{
    Draw, Quad, Renderer, Result,
    text::{GlyphLayout, TextRequest, TextRun},
};
pub struct Page {
    pub quads: Vec<Quad>,
    pub lines: Vec<(GlyphLayout, [f64; 2], [f32; 4])>,
    pub background: [f32; 4],
}
impl Page {
    pub fn new(renderer: &mut Renderer, scale: f64, dark: bool) -> Result<Self> {
        let background = if dark {
            [0.075, 0.085, 0.10, 1.]
        } else {
            [0.96, 0.965, 0.975, 1.]
        };
        let foreground = if dark {
            [0.90, 0.92, 0.95, 1.]
        } else {
            [0.12, 0.15, 0.20, 1.]
        };
        let muted = if dark {
            [0.63, 0.67, 0.73, 1.]
        } else {
            [0.39, 0.43, 0.49, 1.]
        };
        let panel = if dark {
            [0.11, 0.125, 0.15, 1.]
        } else {
            [1.; 4]
        };
        let border = if dark {
            [0.22, 0.25, 0.29, 1.]
        } else {
            [0.85, 0.87, 0.90, 1.]
        };
        let mut page = Self {
            quads: Vec::new(),
            lines: Vec::new(),
            background,
        };
        let mut add_quad = |rect: [f32; 4], color: [f32; 4], radius: f32| {
            let mut q = Quad::new(rect.map(|v| v * scale as f32), color);
            q.radii = [radius * scale as f32; 4];
            q.border_width = scale as f32;
            q.border_color = border;
            page.quads.push(q);
        };
        add_quad([16., 16., 184., 588.], panel, 12.);
        add_quad(
            [28., 96., 160., 38.],
            if dark {
                [0.18, 0.26, 0.38, 1.]
            } else {
                [0.88, 0.93, 0.99, 1.]
            },
            8.,
        );
        add_quad([224., 132., 592., 132.], panel, 12.);
        add_quad([224., 284., 592., 236.], panel, 12.);
        add_quad([224., 540., 592., 64.], panel, 12.);
        add_quad([642., 154., 150., 34.], background, 7.);
        add_quad([642., 208., 150., 34.], background, 7.);
        let mut add_text =
            |label: &str, x: f64, y: f64, size: f64, weight: u16, color: [f32; 4]| -> Result<()> {
                let mut request = TextRequest::new(label);
                request.family = "Noto Sans, Noto Sans CJK SC, sans-serif";
                request.size = size;
                request.weight = weight;
                request.scale = scale;
                let layout = renderer.layout_text(&request)?;
                assert_eq!(layout.missing_glyphs, 0, "missing system glyphs: {label}");
                page.lines.push((layout, [x * scale, y * scale], color));
                Ok(())
            };
        add_text("Plico", 34., 36., 22., 600, foreground)?;
        add_text("SETTINGS", 34., 76., 11., 500, muted)?;
        add_text("Appearance", 42., 105., 14., 500, foreground)?;
        add_text("Text preview", 42., 155., 14., 400, muted)?;
        add_text("Rendering", 42., 201., 14., 400, muted)?;
        add_text("System text / wgpu", 34., 560., 12., 400, muted)?;
        add_text("Appearance", 224., 30., 26., 600, foreground)?;
        add_text(
            "System-shaped text, rendered at the display's pixel scale.",
            224.,
            78.,
            14.,
            400,
            muted,
        )?;
        add_text("Font", 244., 154., 14., 500, foreground)?;
        add_text("System default", 654., 162., 13., 400, foreground)?;
        add_text("Color scheme", 244., 211., 14., 500, foreground)?;
        add_text(
            if dark { "Dark" } else { "Light" },
            654.,
            216.,
            13.,
            400,
            foreground,
        )?;
        add_text("Text preview", 244., 302., 16., 600, foreground)?;
        add_text(
            "12px  Sss 0123456789 Settings",
            244.,
            344.,
            12.,
            400,
            foreground,
        )?;
        add_text(
            "14px  Sss 0123456789 Settings",
            244.,
            376.,
            14.,
            400,
            foreground,
        )?;
        add_text(
            "14px  Sss 0123456789 Settings",
            244.,
            410.,
            14.,
            500,
            foreground,
        )?;
        add_text(
            "中文笔画：清晰、均匀。日本語の小さな文字",
            244.,
            448.,
            14.,
            400,
            foreground,
        )?;
        add_text(
            "Fallback: café e\u{301} · 👩‍💻 · 🇯🇵 · ffi",
            244.,
            483.,
            14.,
            400,
            foreground,
        )?;
        add_text(
            &format!("Device scale: {scale:.2}×"),
            244.,
            560.,
            13.,
            500,
            foreground,
        )?;
        add_text("No extra emboldening", 558., 560., 13., 400, muted)?;
        Ok(page)
    }
    pub fn draws<'a>(&'a self, quads: &'a [Quad], offset: f64, continuous: bool) -> Vec<Draw<'a>> {
        let mut draws: Vec<_> = quads.iter().map(Draw::Quad).collect();
        for (layout, origin, color) in &self.lines {
            let mut run = TextRun::new(layout, [origin[0], origin[1] + offset], *color);
            run.continuous = continuous;
            draws.push(Draw::Text(run));
        }
        draws
    }
}
