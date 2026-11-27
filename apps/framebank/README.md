# Framebank

Framebank 是用 Plico 和 wgpu 构建的本地图片与视频素材库。支持多选导入、搜索、收藏、大图预览、缩放、胶片条、文件属性、取色和按颜色查找相似素材。

```sh
cargo run --locked -p framebank
cargo run --locked -p framebank --features video
```

默认可预览图片和视频封面，也能用系统播放器打开视频。启用 `video` 会链接 libmpv，提供内嵌播放、暂停、拖动进度和静音。导入视频需要 PATH 中的 `ffprobe` 和 `ffmpeg`。程序可从命令行接收一组本地文件路径。导入保留原始文件位置，收藏和路径记录写入平台应用数据目录的 `library.json`。

用 Ctrl 或 Command 加 O 导入，Ctrl 或 Command 加 F 搜索。预览中用左右方向键切换素材、Esc 返回、F 切换收藏、空格播放或暂停视频。

## 预览与录制

```sh
mkdir -p reports/framebank
cargo run --locked -p framebank -- --capture reports/framebank/library.png
cargo run --locked -p framebank -- \
  --scenario apps/framebank/scenarios/inspect.json --capture reports/framebank/inspect.png
cargo run --locked -p framebank -- \
  --scenario apps/framebank/scenarios/showcase.json --record reports/framebank/showcase.mp4
```

## 样片来源

图库样片来自 [Unsplash](https://unsplash.com/license)。视频样片由树林照片生成。

- [Quiet elevations](https://images.unsplash.com/photo-1470770841072-f978cf4d019e)
- [Into the still](https://images.unsplash.com/photo-1473448912268-2022ce9509d8)
- [Tidal study](https://images.unsplash.com/photo-1500375592092-40eb2168fd21)
- [Glass city](https://images.unsplash.com/photo-1511818966892-d7d671e672a2)
- [Road to red rock](https://images.unsplash.com/photo-1500530855697-b586d89ba3ee)
- [After the blue hour](https://images.unsplash.com/photo-1519681393784-d120267933ba)
- [Dune geometry](https://images.unsplash.com/photo-1473580044384-7ba9967e16a0)
- [The understory](https://images.unsplash.com/photo-1441974231531-c6227db76b6e)
