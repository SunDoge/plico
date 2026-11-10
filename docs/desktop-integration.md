# 原生桌面集成

启用 `plico-desktop` 的可选 `desktop-integration` feature，通过
`plico_desktop::desktop::integration(DesktopOptions)` 得到 `DesktopClient` 和
`DesktopExtension`，将扩展交给 `run_source_with_extensions`。

完整独立示例：

```sh
cargo run -p plico-desktop --features desktop-integration --example desktop-integration
```

## 框架负责的行为

- 应用提供 RGBA 图标与原生菜单声明；macOS 支持 alpha template 图标。
- `MenuAction::ShowWindow` 恢复并聚焦指定窗口；`Exit` 终止事件循环；`Emit` 将业务 ID 发给宿主。
- `TrayItem::Toggle` 返回明确的新值，应用决定如何处理与保存。
- close-to-tray 仅在托盘创建成功、宿主可用且策略开启时隐藏指定窗口。其他窗口正常关闭。
- Linux 使用 KSNI／StatusNotifierItem D-Bus 后端，不要求 GTK。没有宿主时正常退出，宿主消失时恢复隐藏窗口。
- `DesktopHandle::notify(id, NotificationRequest)` 返回排队结果，点击后恢复指定窗口并报告相关 ID；失败返回事件。
- 最多一个通知监听任务，忙时返回 `DesktopError::Busy`；系统通知过期是平台提示，不能保证平台按时结束监听。
- 原生回调只传消息并唤醒事件循环。宿主应在 `HostView::poll_updates` 排空 `DesktopClient::try_recv`，隐藏时仍可处理消息。
- 普通通知 effect 和带点击回调的服务共享通知参数到平台的转换逻辑。

`DesktopClient::handle()` 可克隆给宿主异步任务。关闭驻留与通知的即时开关分别是
`set_close_to_tray`、`set_notifications_enabled`。关闭驻留策略后，已经隐藏的窗口会恢复。
扩展的 shutdown/drop 停止宿主探测并注销回调；退出时应用宿主仍须清理自己的服务与秘密。

## 应用负责的行为

品牌图标、菜单文案与 ID、通知内容及隐私、业务动作、审批、偏好文件与错误展示均属于应用。
框架没有密码库、SDK、SSH 审批、登录或账户的概念，平台通知的相关 ID 不会批准任何业务操作。
应用收到成功排队结果也不代表系统已显示通知；应处理 `NotificationFailed`。

本能力不默认启用，不增加普通 GUI 应用的托盘依赖。目前每个进程只支持一个集成实例，
因为原生菜单库的回调是进程级的；第二个实例报告托盘不可用，不覆盖第一个实例。
macOS/Windows 的权限、通知身份、签名／打包与原生视觉仍需各平台验证。
不含开机启动、全局快捷键、原生应用菜单栏和系统屏幕锁定处理。
单实例激活、深链路参数解析与窗口状态恢复由 `plico-desktop` 的常驻 API 提供，
不依赖 `desktop-integration`；URL scheme／文件关联的系统注册仍未实现。

## 验证

```sh
cargo test -p plico-shell --features desktop-integration --lib
cargo clippy -p plico-shell -p plico-desktop --features plico-desktop/desktop-integration --lib --example desktop-integration -- -D warnings
```

框架单元测试覆盖配置拒绝、菜单 ID 唯一性、通知排队上限、停用与 drop、策略更新与事件相关 ID。
独立消费应用的隔离 X11/D-Bus 测试覆盖关闭隐藏、菜单恢复、通知点击恢复、宿主消失与正常退出。
这些测试不等于 macOS／Windows 实机验证，也不保证特定 Linux 桌面已安装托盘扩展。
