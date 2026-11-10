# MyGO 控件样式与组合

参考 MyGO revision `9b26576752f47b8334b67ed60bd4ce0f663cc6de` 的
`ui/theme.go`、`ui/widgets.go`、`ui/textinput.go`、`ui/toggle.go` 和
`ui/toolbar.go`、`ui/paint.go`、`ui/editor_layout.go`、`ui/date.go`、
`ui/popovermenu.go`、`ui/sidebar.go`、`ui/indicators.go`、`ui/tree.go`、
`ui/collapsible.go`、`ui/tooltip.go`、`ui/table.go` 和 `ui/colorpicker.go`。
源码仅作为参考，不作为构建依赖；移植部分的 MIT 许可
保存在 `licenses/MyGo-MIT.txt`。

## 组合边界

Plico 的高级控件由普通 Rust 函数组合基础声明，不另建一套交互和渲染后端：

- `button` 的文字、图标和自定义 children 共用按钮基础行为及 `style_button`。
- `toolbar` 是行布局和按钮外观作用域。内部普通 `button` 自动采用工具栏外观，
  嵌套行列和组件保留作用域；离开工具栏或进入 portal 后恢复外层外观。
- `menu_item` 与 `select` 的选项共用 `popup_entry`，在普通按钮基础上组合
  语义角色、选中面、图标和文字；键盘焦点与指针悬停均采用 MyGO 的强调色高亮。
- `calendar` 直接组合标题按钮、星期行和日期按钮，不再另套卡片外框。
- `segmented` 是轨道和同一套按钮基础的组合。选中段只改变外观，
  禁用、焦点、点击和交互动效继续使用按钮路径。
- `search_field` 由共享输入框外壳、无外观编辑器、放大镜和清空按钮组合；
  外壳和普通输入框共用 `style_input`，裸编辑器不重复添加 padding；
  find bar 等组件继续复用它。表单 field 负责标签、说明和错误，
  不重复实现编辑器。
- 滑杆、复选框、开关的指示器仍由专用绘制代码处理；
  编辑、布局、输入、语义和资源缓存继续由基础控件/后端统一管理。

应用不需要为普通按钮重复指定 radius、边框、hover/pressed 色或 elevation。
有真实布局需求时可以覆盖尺寸；基础控件不是互不相关的视觉模板。

## 默认外观契约

| 控件 | MyGO 对应默认值 |
|---|---|
| 主题 | spacing 4、radius 6；macOS 默认字号 13，其余 14 |
| 普通按钮 | 水平 padding 14、垂直 6；Surface 底色、1px Border；无投影 |
| 主按钮 | Accent 底色、AccentText 文字、独立 hover/pressed 色；无边框和投影 |
| 工具栏 | padding 4、gap 4；内部按钮水平 padding 10；静止透明，hover/press 才显底色 |
| 输入框 | 水平 padding 10、垂直 6；底色保持不变，边框反馈 hover/focus；不叠加通用外圈 |
| 复选框 | 指示器 16×16、文字间隔 8；无额外包围 padding |
| 开关 | 轨道 36×20，白色滑块和轻投影；无额外包围 padding |
| 分段控件 | 轨道 padding/gap 2、radius 6；段 padding 12×4、radius 4；选中段有轻投影 |
| 侧边栏 | padding 10、gap 2；导航项共用 `nav_entry`，选中项在焦点作用域内为强调色，失焦为 SurfacePressed |
| Badge | 水平 padding 6、无垂直 padding；正文色 10% alpha 胶囊，字号为正文减 2 |
| Avatar | 默认大小为字号 × 2.25；FNV-1a 决定 hue，HSL 饱和度 .45、亮度 .55；白色字、正圆裁切 |
| Progress | Border 轨道、Accent 填充，6px 高、3px 圆角 |
| 日历 | 32×28 日期格、2px 行距；选中为 Accent/AccentText；今天为 1px Accent 边框；无额外卡片外框 |
| 菜单/选项 | 垂直 padding 6；菜单水平 8，选项水平 10；高亮为 Accent/AccentText |
| 普通 modal | padding 20、gap 12、radius 10；40% 黑色 scrim；y10/blur30/30% 投影 |
| popover | padding/gap 4、radius 8、1px Border；y6/blur20/18% 投影 |
| Accordion | radius 6、1px Border、裁切；后续项顶部 1px 分隔线；标题 padding 10×12、500 字重 |
| Tree | 行高 28、左右 padding 4/8、层级缩进 16；选中项为 Accent/AccentText |
| Tooltip | 最大宽 320、padding 5×8、radius 5；y2/blur8/20% 投影 |
| Table | 行高 32、单元内容 padding 6×10；选中行为 Accent/AccentText |
| ColorPicker | 宽 240、gap 10；S/V 面板 240×160、16px 色相条、28px 色样、6 列色板 |

`Theme` 的颜色存储仍是 RGB，Focus/Selection 的透明参考色在默认窗口底色上预混合。
这不等于支持任意底色上的同一 RGBA 合成；自定义主题可以覆盖这些 token。
连续圆角仍遵循 Plico 的统一约定，精确圆形指示器保留圆弧。

单行编辑器的文字、选择和光标裁切到边框内侧，不裁切到减去 padding 的内容框。
垂直居中允许负的内容偏移，使实际字体行高能使用 padding；中文或英文下伸笔画
不会因为 padding 增大而截断。多行编辑器继续使用其滚动内容框。
空的裸编辑器仍保留一行可聚焦空间。

## 可重建的验证

运行 `python3 scripts/compare-mygo-widgets.py`，可用 `--mygo` 指定只读参考 checkout。
脚本分别调用 MyGO headless 和 Plico 的生产 wgpu capture，渲染相同控件的明暗主题，以及输入框的静止、hover 和 focus 状态。
MyGO probe 复制到 reports 执行，不修改参考仓库。图片和 reference revision
记录写入 reports，不参与提交，也不作为文档依赖。

核心回归覆盖参考主题数值、基础控件间距、普通按钮无阴影、工具栏作用域恢复，
以及透明按钮的悬停渐入、禁用与焦点。颜色面板的二维拖动使用框架统一的
`pointer_fraction` 变换坐标反馈，runtime 回归覆盖真实指针命中、语义和 RGB 写回；
桌面测试验证组合后仍使用同一交互与语义路径。
两套文字布局和栅格器允许产生文字边缘及行高差异；对照图用于核对外观，
不声称跨后端逐像素相等。

## 尚待完成的核对

本轮已实际渲染核对按钮、输入框（三种状态）、复选框、开关、分段控件、
侧边栏、日历、Badge、Avatar、Progress 的明暗主题。菜单和 select 的键盘高亮
有桌面交互回归，仍需补入跨实现的截图对照。

树列表、折叠面板、表格和颜色选择器现已加入同一组明暗主题双实现截图；验收目标是
结构、状态和功能契约一致，不要求不同文字栅格器与渲染器逐像素相等。Tree 已补齐
单一 Tab 停靠、可见项方向键、父子展开/收起和缓冲前缀选择；Accordion 已补齐组内
Up/Down/Home/End；Sidebar 采用选择项单一 Tab 停靠，方向键与前缀选择会激活目标项，
章节标题不混入选择序列。Tooltip 的显示时序仍由独立宿主测试覆盖。Plico 的 `Color` 目前只有 RGB/token，没有 alpha，
因此颜色选择器没有 MyGO 的透明度滑杆；这是公开颜色模型的能力差异，不以假控件代替。
