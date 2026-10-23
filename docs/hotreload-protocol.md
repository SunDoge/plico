# 热重载协议 v2 与验收边界


状态：native 模块与宿主已实现，普通 JSON Request/Frame 仍用于传输；不是二进制
协议、树增量传输或可同时承载多个 App 的会话注册表。

## 会话与恢复

一个加载映像的 UI 线程保留一个 `App + Engine` 会话。首次加载和显式恢复创建
实例；`resume` 请求继续活跃实例，不用宿主旧 checkpoint 覆盖动画中的新状态。
`lean` 请求继续实例且不输出 App 状态，供动画/滚动驱动使用；节点树仍然全量输出。
事件/服务帧输出新的完整 checkpoint，因此尚未做到“状态只在 reload 时传输”。

`checkpoint` 请求只读取当前状态和树，不调用 show、时钟回调或发送服务命令。
宿主激活候选版本前从旧实例取 checkpoint，避免丢失最后一帧以来的逻辑动画状态。
候选版本输出需通过校验，不允许在预览加载时发送服务命令。

声明失败或 panic 丢弃在途 App/Engine；宿主保留最后成功画面，下一次请求使用完整
checkpoint 和输入快照恢复，不无限重试轻量帧。命令解码失败同样使宿主下次请求
显式恢复。它不是事务回滚：失败前不可逆的外部副作用不可能自动撤销；应用应把
副作用交给宿主命令，整个批次解码成功后再执行。

host、module 和 descriptor 都检查版本。v1 不能静默接受 v2 的会话/输入语义。
CLI 发布 descriptor 使用 `plico_dev::PROTOCOL_VERSION`，不再手写旧版本字面量。

## TextInput

宿主首帧安装模块上报的输入绑定，包括初始值与 placeholder，不等待第一次点击
或动画。模块会话保留 Engine 绑定及响应计数，因此未序列化的 TextBuffer 在普通
事件间仍存在。新会话需要恢复输入时先注册绑定、再应用内容与提交基线，最后
投递点击，避免 Save 先读取旧值。

按逻辑路径传输已提交文本和绝对提交计数；重复计数不重复提交，恢复时区分已确认
基线与尚未投递的提交。一次批次内多个提交按现有 InputResponse 的 bool 语义合并。
更新计数为 O(1)，不按收到的计数逐次循环。模块清空或规范化内容会同步给宿主并
更新确认版本，避免再次回传。传输失败不会提前确认宿主编辑。

Caret、选择与 IME preedit 仅更新宿主呈现；正文版本独立于呈现版本，因此移动
光标不会触发模块 dispatch。宿主自己的 input_revision 驱动实际输入布局重绘。
提交文本仍通过 JSON，Frame 目前也仍携带输入内容；不宣称已实现输入增量编码。

**密码输入在热重载预览中禁用，并输出明确诊断。** secret 不会因绑定回放变成
普通可导出缓冲，也不输出原文。普通 native 路径继续使用原有安全 TextBuffer。
跨模块的密码传输需要单独设计临时数据的生命周期和清零，不能套用普通 String JSON。

## 执行与诊断

native Source 和 module 都调用 `Engine::declare_until_settled`，最多八遍，统一聚合
dirty/changes。View/App 转发使用同一组宏方法，避免包装器和直接 View 实现漏转发。

未消费的路径点击/数值事件计入 Frame，宿主报告数量；这能诊断丢弃，不能检测
位置身份恰好错投到另一个合法控件。跨 revision 输入路由和更详细的分类诊断仍需后续工作。
稳定键哈希不依赖 DefaultHasher 的实现细节，但自动生成调用点编号仍不是业务身份。

panic 回复保留 payload 消息，不安装临时全局 panic hook。命令解码错误包含批次
索引与目标类型，不直接回显 JSON 或可能带敏感值的 Serde 错误消息。
旧映像（包括加载后被拒绝的候选）仍可能保留到进程退出，重载错误会说明这个生命周期。

## 可重建检查

```sh
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p plico-cli -p plico-vello
python3 scripts/check-hotreload.py
python3 scripts/check-text-input-reload.py
python3 scripts/check-modal-reload.py
```

输入脚本经过实际 CLI、cdylib、native adapter 和常驻宿主，验证首帧绑定、中文编辑、
提交幂等、保存、热重载后内容保留和模块清空回传。协议测试另覆盖动画→事件状态连续、
只读 checkpoint、失败恢复及输入与点击顺序。输出日志/截图只作为本地生成产物。
