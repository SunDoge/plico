# 常驻宿主服务与可重载 UI

服务不随界面模块替换。`main.rs` 创建 actor、网络客户端或任务执行器，`lib.rs` 只保留
UI 状态、类型化消息与展示快照。普通运行走直接 Rust 调用，`plico dev` 走值协议和动态模块。
这不需要应用把 SDK 会话、线程或 Future 序列化，也不要求每个组件都有自己的动态入口。

## 运行完整示例

```sh
cargo run -p plico-services-demo
cargo plico dev apps/services-demo
```

点击 Start task，后台 worker 分 50 步执行约 10 秒的模拟任务。运行期间修改
`apps/services-demo/src/lib.rs` 并保存：UI 模块会替换，界面计数和任务继续。
任务不读取文件、不访问网络。Cancel task 通过宿主消息取消工作，结果由 worker 唤醒 UI。

源码分工：

- `src/lib.rs`：`Application`、`TaskSnapshot`、`TaskCommand`、视图与模块导出。
- `src/main.rs`：创建一次 `TaskService`，调用 `run_service_app_with_options`。
- `src/service.rs`：main 的私有模块，拥有线程、channel、取消 token、唤醒回调和业务状态。
  不被 lib 引用，不会编进可重载的 UI 模块。

## 类型化桥接

`plico_dev::services::ServiceApp` 定义 UI 使用的 `Snapshot` 和 `Command`：

```rust
impl ServiceApp for Application {
    type Snapshot = TaskSnapshot;
    type Command = TaskCommand;

    fn show(&mut self, cx: &mut UiContext, services: &mut Services<TaskSnapshot, TaskCommand>) {
        let running = services.snapshot().status == TaskStatus::Running;
        if button(cx).text("Start task").disabled(running).clicked() {
            services.send(TaskCommand::Start);
        }
    }
}
plico_dev::export_service_app!(Application);
```

只读快照和消息 outbox 是当前调用的临时上下文，不属于 Application 的序列化状态。
模块内部仍使用普通函数、泛型、短借用与 children，不保存事件闭包。

同时需要读取大快照并发送消息时，使用 `let state = services.snapshot_shared()`，得到
本地 `Arc<Snapshot>` 的只读句柄，避免每次滚动或动画声明都深拷贝日志和连接列表。
原有 `snapshot()` 仍返回 `&Snapshot`。只有替换快照时创建新的 Arc，旧句柄保持有效；
Arc 不跨动态库 ABI，热重载边界仍传序列化值。

宿主实现 `HostService`：

| 方法 | 职责 |
|---|---|
| `snapshot()` | 返回明确选择的展示数据，不是整个服务对象 |
| `send(command)` | 快速投递类型化消息，IO 留给 actor / executor |
| `poll()` | 消费异步完成结果，返回展示状态是否变化 |
| `set_wake_callback(wake)` | 完成工作后唤醒宿主事件循环 |

这里没有选定 Tokio 或某个 actor 框架；已有 actor handle 可以实现同一个接口。
多个业务服务可以先由一个宿主 facade 聚合快照和消息 enum，尚未引入动态服务注册表。

`ServiceView<Application, TaskService>` 是直接调用路径，类型参数约束快照和消息一致；
`ReloadView::with_services(descriptor, task_service)` 是动态模块路径。
服务对象没有 Serialize 要求。SDK client、channel、线程、输入缓冲等原生资源均可由它持有。
只有显式的 Snapshot 经过跨模块序列化，因此不要把秘密、令牌或完整解密模型加入该展示投影。

## 正常 Rust 项目与开发 CLI

应用仍然是 main.rs + lib.rs：

```toml
[package.metadata.plico]
host = "my-application"

[lib]
crate-type = ["rlib", "cdylib"]
```

`host` 指向同一个 package 的 binary target。该 binary 默认 feature 应包含所需的后端和
`plico-vello/hot-reload`，模块则继续用 `--lib --no-default-features --features abi` 构建，避免链接 renderer 和服务实现。
示例的 `desktop` feature 只用于可执行宿主，默认启用。

main 使用 `run_service_app_with_options(app, services, window_options)`：正常运行创建
ServiceView，开发运行则读取 CLI 提供的 `PLICO_DEV_MODULE` 描述符并创建 ReloadView。
应用不用为这一流程维护自己的参数解析器。
没有 host metadata 的现有应用继续使用框架的通用宿主。

首次 dev 会构建应用自己的宿主 binary，此后只重建和替换 UI library。
**宿主代码、服务实现、服务协议或后端变更需要重启 dev**；不能把运行中的 worker 代码也当作 UI 替换。
通用宿主的 JSON stdin `--headless` 目前没有移植到自定义宿主；自定义宿主有独立的验证入口。

## 提交与恢复边界

1. 请求携带 UI checkpoint、当前服务快照、输入和注入时钟，二者不互相代替。
2. 视图将消息追加到 outbox；同帧稳定化不会因再次声明就重复执行已消费点击。
3. 宿主验证模块输出和整个消息批次。解码失败时保留旧帧，不执行该批次里的任何消息。
4. 有效帧提交后才把消息送到常驻服务；消息从已提交 Frame 中取走，不会在重绘时重放。
5. 异步完成经 `wake → poll → 新快照 → 新视图` 更新界面，空闲时不靠持续 rAF 轮询。

首次展示前会先消费已排队的完成结果；安装 wake 回调时再唤醒一次，覆盖初始化期间的竞态。

加载/激活是预览阶段，禁止候选帧发出服务消息。初始化连接和启动长期服务应放在 main；
用户动作才投递操作消息，避免一次 reload 意外重新登录或重新开始任务。
UI 状态不兼容可以重置；服务快照或消息协议不兼容则拒绝候选模块，服务仍归原宿主所有。

这里保证的是进程内成功接受一次 UI 事件后不因 settling/repaint 重复投递，**不是网络操作的
持久 exactly-once**。服务应自行处理 request ID、重试、过期结果、背压和外部操作的幂等性。
宿主发送消息后产生的外部副作用不随 UI 状态回滚。程序退出或崩溃也不在“重载保留”的承诺内。

输入缓冲虽可放在宿主，当前尚未增加模块视图到原生 TextBuffer 的绑定协议；这个例子验证的是
业务服务与任务桥接，不表示可重载的原生密码输入已经完成。
旧动态库继续保留映射，未改成依赖调用计数的自动卸载。

## 可重复验证

协议测试覆盖快照不进入 checkpoint、消息经过稳定化只返回一次、UI reset 独立于快照、
错误快照拒绝，以及模块 panic 时 outbox 不会发布。
直接调用测试覆盖相同 UI 与类型化服务的按钮、禁用态和取消路径。

真实动态库验证使用受控 worker，不靠 sleep 等待业务进度。先建立两个 UI 版本：

```sh
cargo build -p plico-services-demo --lib --no-default-features --features abi
mkdir -p target/services-verification
cp target/debug/libplico_services_demo.so target/services-verification/original.so
cargo build -p plico-services-demo --lib --no-default-features --features abi,schema-reset-fixture
cp target/debug/libplico_services_demo.so target/services-verification/reset.so
cargo run -p plico-services-demo --features hot-reload --example services-reload -- \
  --module target/services-verification/original.so \
  --reset-module target/services-verification/reset.so
```

上述库名和命令在 Linux 实测；其他平台需要相应 DLL 后缀/复制命令。
验证包括兼容激活、损坏库拒绝、不兼容 UI 重置后 worker 身份/进度仍在、完成与取消。
`schema-reset-fixture` 只用于制造旧 checkpoint 缺字段的测试，正常运行不启用。

另在隔离 X11 中执行了真正的 `plico dev apps/services-demo`：启动任务并增加 UI counter，
保存一次 UI 源码触发 watcher，确认宿主 PID 和 counter 保留，进度条继续变化。
本地截图可用 `cargo run -p plico-services-demo --example services-capture -- --running --out reports/services/running.png`
生成。报告与截图不参与提交。

静态 ServiceView 支持通过 HostService::snapshot_revision 复用 typed snapshot；默认仍为 eager，使用约定与动态路径边界见 [性能说明](performance.md)。

宿主唤醒会在 Source 层合并，服务 poll 应处理可用结果批次，见 [批次边界](batching.md)。


## ABI feature

普通静态应用不需要 stabby。`plico-dev/abi` 才启用 ABI 类型、dispatch 和导出宏；
`plico-dev/host` 包含 abi，再启用动态库加载。应用的 `abi` feature 同时启用
`plico-dev/abi` 和 optional stabby（stabby export 宏的 ABI report 仍需要直接依赖）。
导出调用使用 `#[cfg(all(feature = "abi", not(target_arch = "wasm32")))]`。

`plico dev` 自动为 UI 模块开启 abi，且不启用 desktop；`plico run` 保持正常应用
features。旧应用未定义 abi 时，CLI 对直接依赖 plico-dev 的应用启用 plico-dev/abi。
手动编译可替换模块需要 `--lib --no-default-features --features abi`。
