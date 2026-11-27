# Signal Desk

基于 Plico 和 wgpu 的原生监控界面示例。工作区提供两个演示会话，以及 Overview、
Compute、Network 和 Storage 视图。可暂停样本、切换 RAM／Swap、过滤网络接口，
并在显示设置里切换 Liquid Glass 和不透明表面。收发曲线每秒追加一组模拟流量，
每个会话各自保留最近 60 个点。截图和测试使用固定随机种子。
侧栏可从右上角收起，收起后可从主区左上角展开。

```sh
cargo run --locked -p signal-desk
```

```sh
mkdir -p reports/signal-desk
cargo run --locked -p signal-desk -- --capture reports/signal-desk/overview.png
```
