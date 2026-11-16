# 默认视觉规范

Plico 的组件行为、状态与渲染结构继续以 MyGO 为主要规格；默认视觉纪律参考
[CUI](https://github.com/yappologistic/CUI) 的设计原则，并针对连续圆角、wgpu
绘制和原生桌面宿主重新实现。参考 revision：
`6e6b35326f655405822fcf863fac0d3bb35bb787`。CUI 使用 MIT 许可证，文本见
`licenses/CUI-MIT.txt`。

## 已实现的默认约束

- 所有尺寸落在 4 point 网格上；默认控件高度为 30、36、44。
- 控件、panel、surface 的连续圆角分别为 10、14、18。嵌套表面使用
  `outer_radius - padding` 保持同心轮廓。
- 小、中、大图标分别为 14、16、18，保持偶数逻辑尺寸以减少常见缩放下的
  模糊笔画。
- 普通控件停留在页面上，不默认投射阴影。菜单、popover、dialog 和 toast
  等浮层才使用紧凑接触阴影与宽环境阴影。
- 输入框依靠较强边界识别；按钮使用安静的 hairline。hover 加强边界，focus
  才使用 accent。
- disabled opacity 统一为 45%；hover/focus 的快速反馈统一为 140 ms。
- 页面、panel 和 control 使用不同的中性表面层级，accent 只用于选中、焦点、
  进度和明确主操作。

这些是框架默认值，不禁止应用建立品牌主题。显式尺寸、颜色、材质和
`corner_smoothing(0.)` 仍然覆盖默认值；液态玻璃属于选择性材质，不是普通
控件的默认外观。

## 后续验收

- Gallery 的 controls、forms、popover、dialog、settings 各保留一张确定性
  recipe 截图，检查密度、同心圆角、文字基线、边界和浮层阴影。
- Light、dark 与 compact 三套基线必须使用同一组件树，避免应用层视觉特例。
- 动画继续按用途收口：行选择 100 ms、hover/focus 140 ms、选择状态 180 ms、
  disclosure 320 ms、popover 280 ms、dialog 340 ms、退出 160 ms；每项在宿主
  motion 中实现后才能标记为完成。
