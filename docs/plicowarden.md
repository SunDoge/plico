# Plicowarden 独立应用

原 `apps/warden` 已迁出 Plico，命名为 **Plicowarden**。本地 checkout 位于
`../plicowarden`，与 `plico` 相邻。尚未创建远程仓库。

- 应用、Bitwarden SDK 集成、SSH agent、合成场景和应用 recipe 位于独立 GPL-3.0-only 仓库。
- Plico 保留 Apache-2.0 的通用 Engine、组件、材质、运行时、桌面宿主和 recipe 协议/TS helper；不依赖应用或 Bitwarden SDK。
- 当前应用使用 `../plico/crates/...` 路径依赖本地新 API；这些 API 发布后才能替换为已验证的固定 Git revision。没有指向原 Warden/Wabou 的依赖。

从独立应用目录运行：

```sh
cd ../plicowarden
cargo run --locked -p plicowarden
cargo test --locked --workspace
python3 scripts/run-recipes.py
```

历史实验中的 Warden 名称和数据保留；应用 benchmark 脚本与端到端 recipe 验证脚本也已迁出。Plico 自身只运行框架测试。
