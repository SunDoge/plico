# Cranelift + prebuilt Skia：按 crate 混合编译实验

2026-11-03，基于 `874df83`，Linux x86_64，Intel Core Ultra 9 285K。
本实验使用 rustc 的 Cranelift 后端，不是 Blaze，也不引入动态库、C ABI 或热重载。
正式默认构建配置不变。

## 方法

- 两组都用 `rustc 1.101.0-nightly (3e92e1a2d 2026-11-01)`，LLVM 23.1.5。
- 已安装同一 nightly 的 `rustc-codegen-cranelift` component。
- 保留仓库 dev 配置：`opt-level=1`、`incremental=true`。
- 直接调用该 toolchain 的 Cargo，清空 rustc wrapper，绕过 mbx；依赖预热后才计时。
- 测量完整 Cargo 命令墙钟耗时，包含 Cargo、前端、代码生成以及相应链接工作。
- 每组先预热，再连续做五次源码编辑，每次编辑后构建；另测不修改源码的 no-op。
- Gallery 编辑把 `Motion studies` 改为同长度的 `Motion studie0` 至 `Motion studie4`。
- 核心 crate 编辑是在 `shader.rs` 末尾添加普通公开整数返回函数，逐次改变返回值。
  这是小改动增量场景，不代表重构核心类型后的成本。
- 不同模式按顺序运行，未固定 CPU 核心/频率；小差异不视为可靠优势。
- 库测试只构建指定库；桌面测试构建 Gallery 默认 Skia 可执行程序，包含最终链接。

## 测量结果

五次源码编辑的中位数（秒）：

| 工作负载 | LLVM | 混合 Cranelift | 说明 |
|---|---:|---:|---|
| Gallery UI 库，无默认 feature | 2.616 | 1.021 | 仅 Gallery 切换后端，约快 2.56 倍 |
| plico 核心库的小改动 | 0.899 | 0.924 | 没有观察到加速 |
| 完整 Skia Gallery，修改 UI | 3.768 | 2.185 | 仅 Gallery 切换后端，约快 1.72 倍 |
| 完整 Skia Gallery，修改 UI | 3.768 | 2.206 | Gallery + plico 使用 Cranelift |
| 完整 Skia Gallery，修改 UI | 3.768 | 2.087 | 除 plico-skia 外的已参与 Plico 框架 crate 使用 Cranelift |

最后一组五次范围 2.062–2.100 秒，约减少 45% 构建等待时间。
核心与 Gallery 一起切换没有额外的可靠收益，不能把全部收益归因于核心泛型少。
No-op 构建约 0.39–0.58 秒；表中数字包含这部分固定开销，没有人为扣除。
预热/切换后端产生的重编译不参与中位数；未测量或声称冷构建加速。

## Skia 边界问题

把 `plico-skia` 也交给 Cranelift，会在最终链接时失败。
`nm` 显示它实际引用 `\x01_ZN8SkCanvas8drawRect...`，而预编译 Skia
提供的是不带 `\x01` 的 `_ZN8SkCanvas8drawRect...`。
bindgen 的 `#[link_name = "\u{1}..."]` 是要求抑制符号改名的标记，
当前测试后端把标记保留成了真实符号的一部分。

这与上游 [rustc_codegen_cranelift #1689](https://github.com/rust-lang/rustc_codegen_cranelift/issues/1689)
描述一致。无需拿 Skia 源码重编，也不必给正式依赖打补丁：保留 `plico-skia`
及第三方依赖使用 LLVM，其他 Plico crate 按需切换即可完成混合链接。
`skia-safe` 的内联包装代码可能在调用方实例化，所以只保留 `skia-safe` 为 LLVM 还不够。

可独立复现符号问题的普通 Rust 程序：

```rust
unsafe extern "C" {
    #[link_name = "\u{1}abs"]
    fn absolute(value: i32) -> i32;
}
fn main() {
    assert_eq!(unsafe { absolute(std::hint::black_box(-7)) }, 7);
}
```

相同 nightly、`-Copt-level=0`：LLVM 编译运行通过；增加
`-Zcodegen-backend=cranelift` 后链接失败。

## 复现构建

先安装匹配组件：

```sh
rustup component add rustc-codegen-cranelift --toolchain nightly
```

在独立 checkout 内运行。使用真实 Cargo 可执行文件，避免外层缓存 wrapper：

```sh
cargo_bin="$(rustup which --toolchain nightly cargo)"
# LLVM 基线
RUSTUP_TOOLCHAIN=nightly RUSTC_WRAPPER='' RUSTC_WORKSPACE_WRAPPER='' \
  "$cargo_bin" build --locked -Zcodegen-backend -p gallery --bin gallery
# 只将应用切为 Cranelift
RUSTUP_TOOLCHAIN=nightly RUSTC_WRAPPER='' RUSTC_WORKSPACE_WRAPPER='' \
  "$cargo_bin" build --locked -Zcodegen-backend \
  --config 'profile.dev.package.gallery.codegen-backend="cranelift"' \
  -p gallery --bin gallery
# 框架也切换，保留 LLVM Skia 桥接层
RUSTUP_TOOLCHAIN=nightly RUSTC_WRAPPER='' RUSTC_WORKSPACE_WRAPPER='' \
  "$cargo_bin" build --locked -Zcodegen-backend \
  --config docs/experiments/cranelift-skia.toml -p gallery --bin gallery
```

为每组单独预热，再进行上述五次真实源码编辑、构建计时，并在结束后恢复源码。
构建库时改用 `-p gallery --lib --no-default-features` 或 `-p plico --lib`。
配置方式来自 [Cargo codegen-backend 文档](https://doc.rust-lang.org/cargo/reference/unstable.html#codegen-backend)。

## 正确性与结论边界

LLVM、仅 Gallery Cranelift、Gallery + plico Cranelift、框架 Cranelift + LLVM
Skia 桥接四种配置，都能执行实际 Skia Motion 场景预览。
固定 1120×820、200ms 虚拟时间，四份 PNG 字节完全相同。
混合框架配置另通过 Gallery 控件状态测试 1 项、桌面 Skia capture 测试 3 项。
这验证了当前场景的混合编译、链接与实际绘制，不代表所有平台和 API 都已兼容。

本实验说明按 crate 选择后端已有实际开发收益，不需要先维护稳定 FFI。
泛型数量不是充分判断依据；核心小改动的增量缓存已经很好，而 Gallery 仍有
可观的代码生成工作可以省下。Cranelift 代码的运行效率不能假定与 LLVM 相同；
开发加速与发布性能应分开决策，release 仍保持 LLVM。

## 开发版运行效率代价

另测 Controls 页的 `show + Engine::frame_declarations`，先预热 100 次，再执行
2,000 次更新，每次修改 `app.count`。独立启动程序五次取单次声明中位数；
不含布局、文字整形或绘制，所以不能把倍数解释为整帧或整个应用的 CPU 倍数。

| 编译配置 | 每次声明中位数 |
|---|---:|
| LLVM | 26.48 μs |
| 仅 Gallery Cranelift | 35.76 μs |
| 框架 Cranelift + LLVM Skia 桥接 | 110.22 μs |

仅 Gallery 切换后端，构建等待减少约 42%，本项声明成本增加约 35%。
框架一并切换只再省约 0.10 秒构建时间，本项声明却约为 LLVM 的 4.16 倍。
因此当前更适合只对经常编辑的应用/UI crate 启用 Cranelift，框架与 renderer
继续 LLVM；不建议为了这点编译收益默认把整个框架改为 Cranelift。
是否在真实动画页影响帧率或功耗仍需单独测量。

复现时在实验 checkout 添加 `apps/gallery/examples/codegen_frame_probe.rs`：

```rust
use gallery::{Gallery, Page};
use plico::{Engine, Router};
use plico_dev::App;
use std::{hint::black_box, time::Instant};
fn main() {
    let mut app = Gallery {
        router: Router::new(Page::Controls),
        ..Default::default()
    };
    let mut engine = Engine::default();
    for _ in 0..100 {
        engine.frame_declarations(|cx| app.show(cx)).unwrap();
    }
    let start = Instant::now();
    for i in 0..2000 {
        app.count = black_box(i);
        engine.frame_declarations(|cx| app.show(cx)).unwrap();
        black_box(engine.node(engine.root()).unwrap().children.len());
    }
    println!("{}", start.elapsed().as_secs_f64() * 1_000_000. / 2000.);
}
```

使用上述三种构建配置的 `--example codegen_frame_probe`，每次编译完成后
直接运行产物五次，不通过 `cargo run` 计入启动前的构建时间。
