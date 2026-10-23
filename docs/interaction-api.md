# 交互区域 API（设计草案）

状态：仅设计，尚未导出以下 API。现有后端只维护控件 hover，核心 `Response`
只提供 clicked/enabled；本文不表示 hover 反馈已经接通。

## 声明方式

沿用 context 首参、普通 builder 和 children。区域自身是一个普通布局容器，
不创建额外按钮，不默认添加背景、光标或键盘焦点。

```rust,ignore
let result = (interaction(cx)
        .height(280)
        .width_percent(100.)
        .sense(Sense::Hover | Sense::PointerPosition)).children(|cx| {
        candles(cx, days);
    });

let response = result.response;
let picked = response.pointer_position().and_then(|point| {
    day_at_x(point[0], response.size()[0], days.len())
});
```

`day_at_x`、`candles` 是应用函数。此例不要求每根蜡烛成为交互节点。
实际代码必须处理 size 为零或没有布局反馈的情况。

底层 `.children(...)` 与 children 一样返回
`InteractionResult<R> { inner: R, response: InteractionResponse }`。
这两个类型独立于现有 `InnerResponse`/`Response`，避免给每个普通按钮增加几何负担。
未来按钮可以增加轻量 hovered 查询，但不在第一批实现范围内。

## 第一批接口

```rust,ignore
// 用 bitflags 表达订阅，可组合；默认 Sense::Hover。
Sense::Hover
Sense::PointerPosition

impl InteractionResponse {
    fn hovered(&self) -> bool;
    fn pointer_position(&self) -> Option<[f32; 2]>;
    fn size(&self) -> [f32; 2];
    fn has_geometry(&self) -> bool;
}
```

`PointerPosition` 隐含 hover 观察。`.sense(...)` 是显式订阅，读取方法没有副作用，
不通过“上一次是否调用查询方法”推断订阅。
不订阅位置时 `pointer_position()` 返回 None；未完成布局时
has_geometry=false、size=[0,0]、hovered=false、位置=None。

局部坐标和尺寸均为逻辑像素，原点是区域 border box 左上角；size 为变换前
border box 尺寸。位置由实际呈现变换的逆矩阵计算，包含祖先变换和滚动影响。
裁剪外、不可见、模态遮挡、不可逆变换或指针离开窗口时返回 None。
滚动内容的数据坐标由应用根据明确的滚动偏移另行换算。

区域观察命中目标及其祖先，不以“指针在矩形内”无条件判定 hover。
因此子按钮上的指针也使祖先区域 hovered；不相关的前景浮层遮住它时则不会。
纯 hover 区域不消费点击、不阻止默认滚动、不获得焦点；透明视觉样式本身
不代表事件穿透，沿用宿主的浮层/穿透策略。

## 反馈时序与失效

响应是最近一次提交布局和最新输入的值快照，不持有 Ui 借用，不保留闭包。
它不是本次声明尚未计算出的几何。初次布局完成后，后端发布反馈并按需安排
一次声明，使组件能够读取真实大小；不能同步调用 Taffy 或为每个查询布局一次。

只有 hover 订阅时，进入/离开才使 UI 声明失效。同一区域内移动不重建应用。
订阅位置时，位置变化才失效；多个指针移动在同一宿主帧前合并，声明读取最新值。
已有 hover_background 保留后端样式更新路径，不强迫它经过应用声明。
这只是降低不必要重建，不承诺目前已经具备组件级增量执行。

即使没有指针事件，布局、滚动、transform 动画、模态或浮层变化后也需要重新
命中。反馈没有变化就不再请求帧；有变化则合并为下一批更新。不能在一次提交内
无限重复声明/布局。由 hover 改变自身几何造成的来回震荡，应用应使用稳定区域
包住动画内容；框架不能保证任意这种反馈布局收敛。

hover、坐标、尺寸属于宿主瞬态反馈，不进入应用持久状态；热重载后按有效
实体身份重新命中，实体卸载或身份失效时删除。应用无需手动维护 changed。
动态列表沿用业务 key，静态区域沿用 component 自动身份。

## 后续接口边界

Click、Drag、Wheel 可继续扩展 Sense，但第一批不导出无实现的标志或方法。
拖动需要区分“按下”与“超过阈值开始拖动”，明确 pointer capture、Cancel、
窗口失焦和区域卸载的终止行为；捕获期间局部位置允许在区域外，不能直接复用
仅 hover 时返回位置的约定。多指针需要 pointer ID，不用一个 bool 模拟。
滚轮消费和滚动容器的事件分配要单独设计；观察 hover 不应改变它们。
键盘导航/焦点/无障碍动作也不由 Sense::Hover 隐式赋予。

交互反馈需要进入宿主/UI 的版本化协议，覆盖 native reload 和 WASM split。
应用只依赖核心数据类型，不暴露 Taffy、winit 或 plico-renderer 的类型。
图表比例尺、刻度生成及路径绘制不属于这个交互 API。

## 实现验收

- 虚拟输入验证进入、离开、窗口 Leave，以及仅 Hover 时区域内移动不请求声明。
- PointerPosition 验证最新值合并、border box 原点、缩放/平移/滚动的逆坐标。
- 子按钮继续收到点击，祖先区域可观察 hover；兄弟区域不同时被误判。
- 验证模态、前景浮层、裁剪、动画移走区域及静止指针下的重新命中。
- 首帧未知几何、resize 和卸载清理均收敛，静止后不持续请求帧。
- native、Web 和 split/reload 反馈一致；Candlestick 示例只声明一个绘图区观察区域。
