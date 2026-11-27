# Plico 默认桌面风格

直接使用 `Theme::light()` 或 `Theme::dark()`。背景用 `Color::Surface`，
卡片和设置区用 `panel` 或 `settings_section`，普通操作用 `button`，
每组最重要的操作用 `button(...).primary()`。控件通过语义颜色继承主题，
亮色和暗色布局共用同一套尺寸。

| 主题规格 | 默认 | 紧凑 |
| --- | ---: | ---: |
| 间距单位 | 4 | 4 |
| 普通交互行高下限 | 36 | 30 |
| 正文 / 次级标签 | 13 / 12 | 12 / 11 |
| 控件 / 菜单与面板 / 对话框圆角 | 10 / 14 / 18 | 8 / 12 / 16 |
| 控件图标 | 16 | 14 |

尺寸用逻辑像素。普通按钮、工具栏操作、单行文本输入、搜索框、菜单和下拉的触发器、
复选框、单选框和开关都遵守这条行高基线。较大的文字和自定义内容可以撑高外框。
开关轨道仍为 36 × 20，复选标记和单选标记为 16 × 16，外框负责对齐和点击。
分段按钮内的相邻按钮使用内缩尺寸，玻璃控件自带独立的尺寸和材质规则。

按钮自带悬停和按压反馈，开关滑块、复选标记和单选标记跟随选中状态过渡。
菜单、下拉和对话框自带开合动效。控件会遵守系统的减少动态效果偏好。
侧栏用中性的鼠标选中色，键盘导航时使用强调色。标签页只过渡底部选中线，
切换时文字宽度和点击区保持稳定。

```rust,ignore
use plico::prelude::*;

let theme = Theme {
    metrics: Metrics::compact(),
    ..Theme::dark()
};

// In App::theme, return the chosen theme. In App::show:
let metrics = cx.theme().metrics;
row(cx).gap(metrics.space(2)).align(Align::Center) + |cx| {
    text_input_string(cx, "Project name", &mut project_name);
    if button(cx).text("Save").primary().clicked() {
        save_requested = true;
    }
};
```

`cargo run --locked -p gallery` 打开 Gallery。在 **Controls** 查看默认控件，
用侧栏的 **Compact** 和顶部的 **Dark theme** 切换密度与外观。
**Workspace** 展示实际设置页的组合方式。

按钮和字段沿用 MyGO 的共用外框和交互面，勾选标记与开关轨道保留各自比例。
行高采用 Plico 的默认密度。[MyGO 控件实现](https://github.com/egoist/mygo/tree/74c3f26d8725e839a611e7ce0ef38388a906db20/ui)

自定义外观可以用 `button_base` 组合内容，复用点击、焦点和无障碍语义。
标准 `button` 在这层之上提供默认样式。这个分层也能在 NSH 的基础控件看到。
[NSH Button](https://github.com/longbridge/nsh/blob/224d9a795c7ac9d10d75750de9351adac5549675/crates/nsh-base/src/button.rs)
[NSH Select](https://github.com/longbridge/nsh/blob/224d9a795c7ac9d10d75750de9351adac5549675/crates/nsh-base/src/select.rs)
