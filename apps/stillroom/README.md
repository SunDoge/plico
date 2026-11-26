# Stillroom

Stillroom 是独立的 Plico 桌面素材库，采用图片瀑布流和浮动玻璃预览。它与 Framebank 共用导入、元数据和播放服务，收藏与导入记录分别保存。

```sh
cargo run --locked -p stillroom --features video
```

打开照片后点击画面可围绕点击位置放大，再点击即可回到适应窗口。右侧可切换 Info 和 Similar。下方缩略图与方向键可切换素材，Esc 返回素材库。启用 `video` 需要 libmpv。导入视频需要 FFmpeg 与 ffprobe。

```sh
mkdir -p reports/stillroom
cargo run --locked -p stillroom -- \
  --scenario apps/stillroom/scenarios/showcase.json \
  --record reports/stillroom/showcase.mp4
```

缩略图与预览图按素材 ID 对应。打开动效使用
[MyGO Gallery 的 Magic Move 示例](https://github.com/egoist/mygo/blob/f651b1061d917336f31c3ede523cb49c524172eb/examples/gallery/main.go)
的 450 ms、0.12 回弹参数。返回使用无回弹的 330 ms 弹簧参数，并沿用
[MyGO 的共享元素交叉淡入](https://github.com/egoist/mygo/blob/f651b1061d917336f31c3ede523cb49c524172eb/ui/match.go)，
让缩略图在飞行途中接手。检查面板沿用 MyGO 的先退出、再进入规则。
参见 [MyGO MIT 许可证](../../crates/plico/third_party/mygo-LICENSE)。
