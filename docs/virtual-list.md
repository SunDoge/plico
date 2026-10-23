# 固定行高虚拟列表

`virtual_list` 保留完整的 keyed 数据，只挂载可见行与 overscan 行。适用于日志、Activity 等有界行高列表。

```rust
virtual_list(cx, records, 32.).grow().smooth_scroll(true) + move |cx, item| {
    text(cx, bind(item, |record| record.label.clone()));
};
```

默认 viewport 高 240，overscan 为 2；可使用 `.height(180.)`、`.grow()`、`.overscan(4)`。
每条记录的外层槽位高度固定，内容被裁剪；可变行高、自动测量与按业务 key 滚动尚未实现。

宿主在布局与滚动后计算所需范围。范围变化时，只执行对应的列表操作；重叠业务 key 的行保留节点与绑定，离开窗口的行卸载。应用数据仍保存在 State，行内部的局部节点状态会随卸载丢失，因此需要保留的编辑状态应放在业务数据中。不要把虚拟行当作长期存活的组件。

顶部、底部占位维持完整内容高度，滚动位置由宿主管理。静态与 reload 驱动都在提交帧之前完成窗口反馈与布局收敛。反馈最多执行 8 轮，无法收敛会报告错误。
热重载协议的 Frame 增加可选 `virtual_windows` 字段；宿主与模块应一起更新。普通滚动、虚拟范围未变化的滚动不调用模块；跨虚拟窗口边界时需要一次或多次模块调用以创建/销毁可见行。

这是节点数量优化，不是数据存储优化；完整 Keyed 数据与索引仍在内存中。后端仍布局、遍历当前挂载的树，不能据此宣称所有页面都变成常数开销。

Gallery Lists 页展示 10,000 行，mbx Activity 使用 32 像素固定行高。`tests/virtual_list.rs` 验证节点规模、重叠身份、增量挂载和缩短后的滚动夹取；`tests/reload_protocol.rs` 验证窗口反馈经过序列化协议并且不重新挂载整个应用。
