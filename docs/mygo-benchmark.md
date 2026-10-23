# 下一阶段：对标 MyGo 的完整 UI 体验

目标项目是 [egoist/mygo](https://github.com/egoist/mygo)，重点对照原生 UI，而非 webview。对标目标不是复刻全部控件名称，而是用正常应用代码实现好看、可组合、能持续演进的界面。

MyGo 的公开文档将 flex/grid 约束、主题颜色和度量、无外观的交互 base 与默认外观组件组织为完整体系。[布局](https://github.com/egoist/mygo/blob/main/docs/ui/layout.md)、[主题](https://github.com/egoist/mygo/blob/main/docs/ui/styling.md)、[自定义组件](https://github.com/egoist/mygo/blob/main/docs/ui/custom-widgets.md)。其 transition 文档还明确位置/尺寸/颜色、进出和共享元素的语义；Plico 目前的属性 tween 只覆盖其中一部分。[Transitions](https://github.com/egoist/mygo/blob/main/docs/ui/transitions.md)。这些是文档所述能力，不是本仓库已实测的完成度或性能比较。

## 三个验收维度

- 好看：统一的语义色、字号/字重、间距、边框、圆角、密度与 motion；浅/深色、焦点、hover、disabled、selected 同属一套规则，页面不各写一套。
- 好写：应用表达内容、状态和操作。常见页面由导航、表单、列表、工具栏和面板组合，不要求开发者反复复制节点树、颜色值和状态判断。
- 好维护：行为基础处理 pointer、键盘、focus 和无障碍；组件层负责外观；业务层负责模型与操作。改主题或组件默认值，不必逐页修改。

## 下一条完整切片

选择一个项目工作台：侧栏导航、项目列表、选中详情、设置与弹层。用同一份业务内容对照 MyGo 的表达方式和效果。任务路径包含选择项目、修改偏好、保存反馈、空列表与错误恢复，而不只是摆一墙控件。

这条切片现在已落实项目选择、独立偏好草稿、保存/重置、归档与恢复，以及公共 Theme/Metrics 和 icon + 文本按钮。导航、项目卡片是普通 Context 首参函数，panel 是应用自己的 builder，不需要给 Context 增加控件方法。弹层、输入、滚动仍未接入。
每一步同时交付应用代码、视觉样本和行为验证。评估指标是页面里多少样式重复、组件修改影响几个文件、窗口缩小时内容是否仍可用，以及键盘操作能否完成同一任务；性能使用同场景同机器测量，不拿各自独立报告直接比较。

## 当前边界

当前 gallery 已有 Workspace / Layout / Controls / Motion 四页；私有 Palette 已移除，使用公共语义色与度量。按钮支持子内容、禁用继承和父按钮语义，keyboard/pointer 路径有真实后端测试。但这还不是完整设计系统；typed Page 也不是完整 router。
Rust 类型、children、编译期身份与 host 动画是 Plico 可以发挥的条件；是否得到更好的 DX，需要以上应用切片给出证据。


## 本机源码对照（2026-10-19）

只读参考 `/home/me/code/go/mygo`，提交 `7e8688284e4b1fc752169ed9db5f51a5f213a2e6`，未修改该仓库。本轮阅读了 ui/frame.go、ui/api_gen.go、ui/base.go、ui/element.go、ui/theme.go、examples/gallery/main.go 和 docs/ui 的 custom-widgets/styling 文档。

| MyGO 的设计 | Plico 的取舍与实际进度 |
| --- | --- |
| Context 管构建环境，独立函数返回 Element | 新增 UiContext 与 widgets::button/text/column 等函数；普通函数是主要应用组件入口。Ui 原方法兼容保留。 |
| Element 为池化节点的短期句柄，运行时检查 build generation | 保留借用 Context 的真实 Rust builder，不引入每控件 RefCell/锁。配置结束时提交；还不能像通用 Element 那样创建后任意交错修改。 |
| Children 暂时改变 Context.parent | 保留同步 children 与显式重新借用；component 支持合法 Rust 的双分号简写，view 宏已移除。 |
| 行为 base 与主题外观分开 | button_base 已支持组合、焦点、语义与禁用继承；其余多 part 控件尚未迁入。 |
| 颜色和 spacing/font/radius 来自同一个主题 | 公共 Theme/Metrics 同时用于默认控件与工作台，light/dark/compact 可实测。 |
| 自定义 widget 是普通函数，可返回 Element | nav_item/project_tile 使用普通函数，panel 返回自定义可配置容器，不强制 Widget trait。 |
| 动态身份需要 key 在读取控件状态前就存在 | component 为静态声明生成词法身份，动态列表显式业务 key。仍不保证源码重构后的所有身份稳定。 |
| 状态直接决定样式，不要求选择器系统 | 继续普通 if/函数和语义 token；host 承担属性动画，应用侧 hover/focus 查询还没有完整接口。 |

Plico 的两个重点是宏减轻静态 key 的负担，以及常驻宿主下的 best effort 应用模块热重载。后者已验证 schema 不兼容时重置，不把“Rust 天生更容易热重载”当作已证实结论；编译时间、ABI/宿主边界和状态迁移仍需要持续测量。

后续优先顺序是受控输入与文本编辑、滚动容器及裁剪、popover/dialog 的焦点和关闭规则，再据实际用法提取更多行为基础；不先复制完整控件名单。text! 已先解决动态文字临时 String 分配，详见 [文字格式化](text-formatting.md)。
