# GridCraft API Edition · 字体保真与 XLS 支持

版本 `0.12.1-api.20261027.2`，基于上游 v0.12.1 的定制版，非官方发行版。

- 缺失中文字体按宋体/黑体/楷体/仿宋设计选择系统替代，状态栏显示替换详情；保存保留原字体名称。
- 保留 XLSX 单元格分段文字的字体、字号、颜色等；同文不同格式不再合并。未改文字编辑保留分段格式，整格字号/粗体命令同步作用于分段。
- 中英文采用稳定行框，自动行高使用显示字体测量；不强制改变原自定义行高、字号或单元格对齐。编辑器遵循居中/右对齐，保留多行换行。
- 保留第三方 API 设置、选区问答、三种翻译、汉化、图片和剪贴板修复。

## XLS（Mac / Win64）

先将 [官方 LibreOffice](https://www.libreoffice.org/download/download-libreoffice/) 安装到系统默认位置，然后在 GridCraft 直接打开 `.xls`，本机转换后自动显示。不上传或覆盖原文件，不静默切换为丢失格式的内置读取。找不到转换器会提示安装。不执行宏、不更新外部链接。编辑后另存 `.xlsx`；不支持 `.xls` 写回，宏及部分旧对象不保证保留。LibreOffice 不包含在下载包内。

## 下载

Apple 芯片 Mac：`macos-arm64.zip`；Windows 10/11 x64：`win64.zip`，完整解压运行 EXE。Windows MSVC 构建需要 [Visual C++ x64 运行库](https://aka.ms/vc14/vc_redist.x64.exe)。附完整源码 ZIP 和 SHA256；旧 Release 保留。

回归覆盖分段字体读写/重开、字体命令/撤销、行高、自定义高度保护、对齐，以及原图片、剪贴板和 API 测试，另有真实 LibreOffice 隔离转换测试。详细范围见源码 `gridcraft/docs/local-document-fidelity.md`。

不宣称与 Excel 完全相同：列宽仍采用默认字符宽度估算，复杂图表、打印分页、部分上下标和缺失字体仍可能不同。没有本次 Windows 桌面实机验证，没有调用付费 API。包内不含用户文件或密钥。Mac 临时签名、未公证；Windows 未做发布者签名。
