# WordCraft 0.11.0 中文版 · 字体与图片修复

版本 `0.11.0-wordcraft.20261027.1`，基于上游 v0.11.0 的定制版，非官方发行版。

- 缺失宋体、黑体、微软雅黑、等线、楷体和仿宋时按设计匹配系统/开源替代；保存仍保留请求字体名。不捆绑微软字体。
- 修复 DOCX 图片负裁剪在读取/保存时被清除；位图外扩空白不再被边缘像素拉满。PNG/EMF 的正/负裁剪有像素检查；内嵌/浮动图片有保存重开回归。
- 保留汉化、中英翻译和此前 UTF-8 正反选区、表格取值修复。WordCraft 仍使用 `http://tech0.top:37002/translate_text`，不是 GridCraft 的自定义 API 版。仅点击翻译时发送候选文字，HTTP 不加密，请勿发送未获授权的敏感内容。

Mac Apple 芯片下载 `macos-arm64.zip`；Windows 10/11 x64 下载 `win64.zip`，完整解压运行。Windows MSVC 版需要 [Visual C++ x64 运行库](https://aka.ms/vc14/vc_redist.x64.exe)。附完整源码 ZIP 和 SHA256，源码分别在 `wordcraft/`、`craft-i18n/`。

没有收到具体 WordCraft 故障样本，不能保证所有字体、图片和排版与 Word 相同，尤其 SVG、EXIF 旋转、复杂浮动对象仍可能有限制。没有本次 Windows 桌面实机验证，没有上传用户文档。Mac 临时签名、未公证；Windows 未做发布者签名。包内无用户文档、配置或凭据。
