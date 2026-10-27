# 本机 XLS 兼容补丁 1

更新：2026-10-27。基于 GridCraft 0.12.1，官方源提交
`f86aec12dabc744b5f75e1033f87bfe685e78481`。这是用户本机定制补丁，不是官方发布版。

## 架构与使用

- GridCraft 没有可安装 LibreOffice 扩展的插件接口。本补丁使用独立、官方签名的
  LibreOffice 可执行程序做 XLS→XLSX 转换；没有复制、链接或打包 LibreOffice 源码。
- 为 macOS 和 Windows 安装转换服务；其他平台与网页端保留原行为。
- 使用原来的文件选择器、最近文件、拖放、Finder“打开方式”或启动参数打开 `.xls`。
  `.xlsx` 等其他格式仍走 GridCraft 原有读取流程。
- 导入结果没有保存目标；“保存”要求另存 XLSX。临时文件不会作为保存目标或最近文件。
- `.xls` 过滤器原本已存在。本补丁改善的是导入保真度，不是声称从零新增 XLS 支持。
- 已补充 macOS 应用包的 `.xls` 文件类型登记（`com.microsoft.excel.xls`）。
  用户确认后，仅把 XLS 的系统默认打开程序设置为 GridCraft，双击即可自动转换并打开；
  XLSX、XLSM、XLSB、XLT、CSV、TSV、ODS、DOC/DOCX、PDF 的默认程序保持原状。
- 字体仍由本机系统提供；格式、公式、打印布局及复杂对象不能保证与 Excel 完全一致。
  宏不执行、不保留。密码保护、原始 BIFF2–4、伪装成 XLS 的 HTML 不在此入口支持范围内。

## 依赖及安全

安装 LibreOffice 到 `/Applications/LibreOffice.app` 或 `~/Applications/LibreOffice.app`。
Windows 需将官方 64 位 LibreOffice 安装到默认 Program Files 目录；也检测 LOCALAPPDATA 下的 LibreOffice/Programs/LibreOffice 目录。Windows 无 sandbox-exec 网络隔离，取消时只结束本任务的进程树。

验证使用官方 macOS arm64 LibreOffice 26.2.6.3（发行版 26.2.6）。
下载自 https://download.documentfoundation.org/libreoffice/stable/26.2.6/mac/aarch64/LibreOffice_26.2.6_MacOS_aarch64.dmg
SHA256：`94bb3248df074c225490a8a6d1d9dc87c7d6783dbb7a8e9f0d0c3d94348552af`。
其签名为 The Document Foundation (7P5S3ZLCN7)。没有调整系统 Gatekeeper 设置。

- 仅传递私有临时文件副本；每次新建临时用户配置，不触碰用户的 LibreOffice 配置。
- 禁用宏、活动嵌入内容、DDE、自动更新链接；不信任任何文档位置。
- macOS sandbox-exec 拒绝 IP 网络连接；保留系统所需 Unix IPC。
  沙箱启动失败时直接报错，不取消隔离后重试。
- 使用参数数组调用，不经 shell 拼接用户路径。Unicode、空格及引号路径支持。
- 单次最多 256 MiB 输入/输出，90 秒转换超时；退出时取消当前任务、回收子进程并清理临时目录。
- 后台转换不会阻塞界面。转换失败显示错误，不悄悄降级到丢失样式的内置导入。
- 极端情况下强制杀进程或断电可能残留系统临时目录，原文件仍只读。

## 自动化接口

`app.open {path:"/absolute/file.xls"}` 返回 `{pending:true,state:"running"}`。
随后通过 `engine.execute` 调用 UI 命令 `file.importXls.status {}`，等待
`state` 变为 `succeeded` 或 `failed`；只有 succeeded 表示已加入工作簿。

直接调用 `file.importXls {path}` 同样是异步导入，不打开交互对话框。
缺少 path 返回错误。一次只处理一个兼容转换，忙时明确报错。
低层引擎 `file.open`、CLI 和 MCP 仍为原内置同步解析器，不启动外部程序。
若要临时恢复原来的界面导入方式，可在启动前设置 `GRIDCRAFT_BUILTIN_XLS=1`。

## 构建及复原

Rust 1.95.0，`cargo build --release -p gridcraft`。
本地构建通过 sibling `../craft-libs` 使用未修改的 craft-libs，固定提交
`d27646a5f7c8b4fc85d9dd3213b8c88b58e0088d`；不要替换成其他提交。
本机无 craftrules/plan 私有文档，遵循可用的仓库 AGENTS.md。
没有添加供应商字体/图片或客户样本到源代码仓库。

原应用与设置先备份，再替换原 `/Applications/GridCraft.app`。
补丁保持 bundle ID 和配置目录，因此现有语言、最近文件和设置保留。
本地修改版使用 ad-hoc 签名，不冒充官方开发者签名。原版备份保留官方签名。
恢复：退出 GridCraft，将备份中的 GridCraft.app 复制回 /Applications。
官方自动或手动更新可能覆盖补丁；请保留补丁包和原版备份。

## 测试范围

新增测试：合成 XLS 的真实 LibreOffice 往返、中文/空格/引号路径、数值及公式、
粗体和合并区域、原件不变、非 XLS 拒绝、取消、缺失输出、转换超时、
异步成功/失败、保存目标为空、最近文件指向原件、缺少参数不弹窗。
另用用户附件做私有验证；测试文件、数据和截图不随源代码分发。
交付说明中记录实际运行的检查，不将局部验证表述为完整 CI 或跨平台测试。
