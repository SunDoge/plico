# 路线图

路线图只记录当前未完成的方向。已经落地的能力放在[实现状态](status.md)，历史实验放在
`docs/experiments/`；不能把早期计划继续写成当前缺口。

## 当前基线

- 稳定调用点/业务 key、局部与持久状态、失败帧恢复及一次性事件消费已经进入 Engine。
- 原生文本输入、选择、剪贴板、IME preedit/commit 和宿主 caret 几何已经进入默认桌面路径。
- 注入时钟、确定性动画、宿主 wake、任务 generation/cancel 和静态编译重启已经落地。
- Gallery、launcher、Plicowarden 等真实应用已覆盖搜索、动态列表、编辑、异步服务和主题。
- JSON/TypeScript recipe、离屏截图与批量场景是当前开发路径；cdylib/Stabby 热重载已移除。
- 固定/可变行虚拟列表已有稀疏测量、业务锚点、FollowEnd 和可打断动画。

## 1. 扩大正确性证据，而不是继续堆缓存

保持全量 geometry/Scene oracle，扩大到随机输入序列、嵌套滚动、modal/portal、动画中重排、
设备丢失与 DPI 变化。Linux 真实窗口持续验证；macOS 由手动 CI 和用户机器验证。性能修改必须
同时给出局部路径计数与最终像素/语义对照，不能只凭节点访问数宣称收益。

验收：同一 seeded 序列的增量与强制全量输出一致；失败能保存 recipe、seed 和阶段统计；
真实窗口无逐帧资源增长、无滚动越界或交互状态丢失。

## 2. 测清应用声明与 Scene/GPU 边界

renderer 的 instance 生成、编码和单批 submit 已有独立计时；应用 declaration/settle、retained
sync、Taffy/flatten、Scene 构建与完整复用现在也进入同一原始帧样本。renderer 样本同时记录
isolated layer/effect/backdrop 的 pass、纹理字节和实际 scissor 像素，并随 recipe capture 报告
序列化。下一步用这些证据减少玻璃/backdrop 的着色像素和离屏面积；只有证据指向 CPU 时才
增加新的缓存层。

验收：普通控件、滚动列表、玻璃和动画四类 recipe 都能导出稳定阶段预算；优化前后使用同一
adapter、分辨率和像素 oracle；空闲帧停止，局部更新不退化成无依据的全量遍历。

## 3. 完成跨平台输入、文本与无障碍边界

补真实输入法候选窗口证据、Windows/macOS 文本质量验证、系统语言探测和自动 RTL。公开必要的
失焦/按键编辑钩子后，再实现 Decimal input 和 Token field；不在核心事件能力缺失时用近似控件
冒充。语义树继续覆盖 modal、虚拟化行、动态提示和焦点恢复。

验收：多输入框重排不丢 composition/selection；候选窗口跟随 caret；键盘与辅助技术能访问
虚拟化内容；RTL 同时影响布局、方向键和文本，而不只是镜像截图。

## 4. 强化确定性 recipe 工作流

继续以 Bun 管理 TypeScript fixture，由脚本组合公共状态并生成宿主消费的 JSON。已有确定性的条件等待；继续补
异步服务完成/失败、语义断言、截图基线元数据和一次编译运行多 recipe 的报告；应用状态类型仍由
应用定义，不要求 `App` 整体序列化。

验收：登录/同步/失败重试、列表滚动、dialog、主题和 reduced-motion 可无人工操作重放；失败报告
明确停在哪个步骤并保留可复现输入；同一 executable 批量执行场景，不为每个状态重新编译。

## 5. 提炼嵌入式宿主边界

从默认桌面路径提炼输入、时间、场景和资源协议，再扩展 Bevy、FLTK、LVGL 等实验宿主。优先共享
运行时和组件语义，不要求转换成宿主原生控件，也不让实验后端限制默认 wgpu 架构。

验收：同一组件行为在桌面与至少一个嵌入 surface 一致；输入不穿透；销毁释放资源；无动画时
不持续更新；不支持的控件在创建前明确拒绝。

此顺序是默认优先级，不阻止明确的应用需求；任何新方向都要先说明它属于已实现能力、验证扩展
还是设计目标。
