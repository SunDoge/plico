# 宿主服务与 UI

`main.rs` 创建 actor、网络客户端或任务执行器，UI 通过类型化消息和展示快照使用它们。所有调用是静态 Rust 调用，不加载动态模块。`plico dev` 重编译后重启整个应用，包括服务；需要恢复的业务数据由应用正常保存。

```sh
cargo run -p services-demo
cargo plico dev apps/services-demo
```

示例点击 Start task 后，后台 worker 分步推进，完成时唤醒 UI。Cancel task 通过消息取消工作。服务层不借用 UI 状态，不阻塞 UI 线程。

```rust,ignore
impl ServiceApp for Application {
    type Snapshot = TaskSnapshot;
    type Command = TaskCommand;
    fn show(&mut self, cx: &mut UiContext, services: &mut Services<TaskSnapshot, TaskCommand>) {
        if button(cx).text("Start task").clicked() {
            services.send(TaskCommand::Start);
        }
    }
}
```

普通 `App` 和 `ServiceApp` 不强制序列化。服务对象也不要求 Serialize，可以持有 SDK client、channel、线程等资源。当前 Snapshot/Command 仍提供值协议能力，用于 WASM 拆分宿主；不要把密钥或完整解密模型放进展示快照。

| HostService 方法 | 职责 |
|---|---|
| snapshot() | 提供展示数据 |
| snapshot_revision() | 可选版本号；避免无变化时重建快照 |
| send(command) | 快速投递消息；IO 留给 worker/executor |
| poll() | 消费完成结果，报告快照变化 |
| set_wake_callback(wake) | 异步完成后唤醒事件循环 |

读取大快照并发送消息时，`snapshot_shared()` 返回本地 Arc，避免深拷贝。快照替换不影响旧读句柄。`ServiceView` 统一负责投影、命令 drain 和唤醒；`run_service_app_with_options` 直接启动这个适配器。

Future 任务使用 `plico_dev::tasks::Task` / `TaskService` 和调用方执行器，暴露 Idle、Pending、Ready(Result)、Cancelled 等状态。原生阻塞工作可使用 `plico_runtime::background::Worker`，保留 worker-local 模型或客户端，支持替换请求、过期结果过滤和合并唤醒。取消是合作式的，不代表已发出的网络请求或磁盘操作已撤销。

预览时通过 `PreviewScenario` 的工厂构造假服务或固定结果，不必加载真实模型、重新登录或调用付费 API。详见[场景预览](preview.md)。
