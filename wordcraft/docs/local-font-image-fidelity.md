# 中文字体与图片兼容更新

更新：2026-10-27。定制版 0.11.0-wordcraft.20261027.1。

缺失中文字体时按宋体/黑体/微软雅黑/等线/楷体/仿宋设计选择系统或开源替代，精确安装字体优先，不改变保存的字体名，不打包微软字体。

DrawingML srcRect 负值表示扩展源图窗口。原读取、保存和 crop_window 夹到 0..1，丢掉外扩空白并错误放大图片。现在保留有界 ±10，异常输入仍保护、窗口至少留 0.5%。位图只绘制源图与框的交集，避免边缘像素拉伸。EMF 使用相同 crop_window。

回归：`fonts/src/word.rs` 中文设计/别名；`docx/tests/it/roundtrip.rs::images_round_trip` 内嵌/浮动负裁剪；`metafile/src/tests.rs` 窗口坐标及异常输入；`render/src/lib.rs::a_crop_keeping_a_sliver_fills_the_frame_with_it` PNG/EMF 红色内容和外扩空白像素。

保留原翻译取值修复，见 `local-pcbtool-translation.md`。构建用仓库 `craft-i18n/`，不依赖开发机目录。没有用户 WordCraft 故障样本，不能保证其他图片全部解决，SVG、EXIF 旋转、复杂对象/分页仍可能不同。测试不上传文档。

修订记录：2026-10-27，中文字体与图片负裁剪读写、渲染修复。
