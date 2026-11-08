use serde::Deserialize;

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Options {
    title: String,
    width: u32,
    height: u32,
    min_width: u32,
    min_height: u32,
    max_width: Option<u32>,
    max_height: Option<u32>,
    resizable: bool,
    decorations: bool,
    centered: bool,
    corner_radius: u32,
    background: plico_renderer::platform::WindowBackground,
    window_level: plico_renderer::platform::WindowLevel,
    input_mode: plico_renderer::platform::WindowInputMode,
    activate_on_show: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            title: "Plico × C#".into(),
            width: 1040,
            height: 780,
            min_width: 640,
            min_height: 480,
            max_width: None,
            max_height: None,
            resizable: true,
            decorations: true,
            centered: false,
            corner_radius: 0,
            background: Default::default(),
            window_level: Default::default(),
            input_mode: Default::default(),
            activate_on_show: true,
        }
    }
}
pub(crate) fn parse(json: &str) -> Result<plico_desktop::WindowOptions, String> {
    if json.len() > 65536 {
        return Err("window options exceed 64 KiB".into());
    }
    let p: Options =
        serde_json::from_str(json).map_err(|e| format!("invalid window options: {e}"))?;
    for value in [
        Some(p.width),
        Some(p.height),
        Some(p.min_width),
        Some(p.min_height),
        p.max_width,
        p.max_height,
    ]
    .into_iter()
    .flatten()
    {
        if !(1..=8192).contains(&value) {
            return Err("window dimensions must be 1..8192".into());
        }
    }
    if p.max_width.is_some_and(|v| v < p.min_width)
        || p.max_height.is_some_and(|v| v < p.min_height)
        || p.corner_radius > 1024
        || p.title.len() > 4096
        || p.title.contains('\0')
    {
        return Err("invalid window constraints or title".into());
    }
    let mut options = plico_desktop::WindowOptions::new()
        .title(p.title)
        .initial_inner_size(p.width, p.height)
        .min_inner_size(p.min_width, p.min_height);
    if p.max_width.is_some() || p.max_height.is_some() {
        options = options.max_inner_size(p.max_width.unwrap_or(8192), p.max_height.unwrap_or(8192));
    }
    options.resizable = p.resizable;
    options.decorations = p.decorations;
    options.centered = p.centered;
    options.corner_radius = p.corner_radius;
    options.background = p.background;
    options.window_level = p.window_level;
    options.input_mode = p.input_mode;
    options.activate_on_show = p.activate_on_show;
    Ok(options.normalized())
}
#[cfg(test)]
mod tests {
    #[test]
    fn validates_and_maps_constraints() {
        let p = super::parse(r#"{"width":500,"minWidth":300,"maxWidth":400,"decorations":false,"windowLevel":"alwaysOnTop"}"#).unwrap();
        assert_eq!(p.initial_inner_size.0, 400);
        assert!(!p.decorations);
        assert_eq!(
            p.window_level,
            plico_renderer::platform::WindowLevel::AlwaysOnTop
        );
        assert!(super::parse(r#"{"width":0}"#).is_err());
        assert!(super::parse(r#"{"maxWidth":100}"#).is_err());
        assert!(super::parse(r#"{"unknown":true}"#).is_err());
    }
}
