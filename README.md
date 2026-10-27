# GridCraft API Edition

在 GridCraft v0.12.1 上增加可配置的第三方 API 翻译和选区问答。这是定制版本，不是 Storytold 的官方发行版。

新版 `0.12.1-api.20261027.3` 增加 AI 智能图表（推荐、预览、确认插入原生图表）和选区数据分析。右上角“AI 图表 / 分析”进入，沿用现有 API 设置。详见 [智能图表与数据分析](gridcraft/docs/local-ai-insights.md)。以对应 Release 的构建状态为发布依据。

本仓库保留先前的中文界面、XLS 转换、图片显示与剪贴板图片修复。GridCraft 源码在 `gridcraft/`，本地依赖在 `craft-libs/`。WordCraft 中文修复版在 `wordcraft/`，翻译目录在 `craft-i18n/`，使用独立 Release 和安装包，`0.11.0-wordcraft.20261027.2` 起移除固定网站调用，改由用户自行填写 API 信息。

2026-10-27 保真更新：按中文字体设计选择替代并提示；保留单元格分段字体/字号/颜色；自动行高使用真实显示字体测量，自定义行高和原单元格对齐不强制改变。详见 `gridcraft/docs/local-document-fidelity.md`。

## XLS 支持（Mac / Win64）

先安装 [官方 LibreOffice](https://www.libreoffice.org/download/download-libreoffice/) 到系统默认位置，然后在 GridCraft 直接打开 `.xls`，本机转换后打开，不上传或改变原文件。编辑后另存 `.xlsx`，不支持 `.xls` 写回。找不到转换器会明确提示，不静默改用丢失格式的读取。宏不执行，宏及部分旧对象不保证保留。LibreOffice 不随包提供。

## WordCraft API 版

在 Releases 选择 WordCraft 独立发行版，改进中文字体替代和 DOCX 图片负裁剪读写/边缘拉伸，保留中英翻译及选区取值修复。下载 `WordCraft-…-macos-arm64.zip` 或 `WordCraft-…-win64.zip`。

开始功能区点击“API 设置”，选择服务商并填写地址、模型 ID、API Key。支持 GPT / OpenAI、MiniMax、DeepSeek、Kimi、小米 MiMo 及自定义兼容接口；三个翻译按钮共用设置，不再调用固定网站。候选文字按范围发送，生成独立副本；不自动重试，拟跳过项可能复核一次。详见 [WordCraft API 使用说明](wordcraft/docs/local-api-assistant.md)。

构建：设 `CRAFT_I18N_DIR` 为本仓库 `craft-i18n/` 绝对路径、`CRAFT_I18N_REQUIRED=1`，在 `wordcraft/` 运行 `cargo build --locked --release -p wordcraft`。Cargo 获取公开固定版 craft-libs。打包使用 `scripts/package.py --app wordcraft`。

## 使用

1. 解压完整软件包，打开 GridCraft。Windows 10/11 x64 运行 `GridCraft.exe`；Mac 打开 `GridCraft.app`。
2. 单击窗口右上角“API 设置”，选择服务商，填写该平台的 API Key 和账户可用的模型 ID，核对 API 地址后保存。预设模型只是可修改的起点，不代表账户已获该模型权限。
3. 框选单元格，单击右上角“AI 选区问答”，在右侧输入问题并发送。拖动面板左边界调宽；“放大 / 缩小”切换宽度，“—”最小化为右侧 AI 按钮，再点 AI 即可恢复。
4. 原有三个翻译按钮共用当前 API 设置：选区转英文、全工作簿转英文、选区转中文。译文在新副本中打开，保留原文、格式及已有完整双语。

打开文件、框选和输入问题不会自动发送数据。只有“发送问题”、“测试连接”或翻译按钮会调用接口；连接测试不发送表格内容。问答不会自动修改工作簿，也没有执行命令、联网或操作文件的工具。

## 接口支持

| 预设 | 默认 Base URL | 协议 / 密钥 |
| --- | --- | --- |
| GPT / OpenAI | `https://api.openai.com/v1` | Responses / Bearer，可改为 Chat Completions |
| MiniMax | `https://api.minimax.io/v1` | Chat Completions / Bearer；国内账号可改为 `https://api.minimaxi.com/v1` |
| DeepSeek | `https://api.deepseek.com/v1` | Chat Completions / Bearer |
| Kimi | `https://api.moonshot.cn/v1` | Chat Completions / Bearer |
| 小米 MiMo | `https://api.xiaomimimo.com/v1` | Chat Completions / `api-key` |
| 自定义 | 自行填写 | OpenAI 兼容的 Chat Completions 或 Responses |

支持 Base URL 或以 `/chat/completions`、`/responses` 结尾的完整端点。高级设置可更改认证方式、超时及额外 JSON 请求参数，适配区域端点和第三方网关。此版本不声称支持每家平台所有独有接口；它支持上述兼容协议。

API Key 默认仅保留在进程内；勾选记住后，Mac 使用系统钥匙串，Windows 使用凭据管理器。JSON 配置不保存明文 Key，密钥不进入工作簿、发布包、命令行参数或源码。配置路径是 GridCraft 的系统用户配置目录内 `ai-api.json`。聊天记录仅保留在当前会话，切换工作簿时清空，避免不同文件混用上下文。

默认使用 HTTPS，不跟随重定向；本机接口可用 HTTP，远程 HTTP 需在高级设置显式允许。网络错误和 HTTP 错误不自动重试。GridCraft 翻译遇到模型格式错误时最多请求一次纠正，WordCraft 不自动修复重试；拟跳过的双语条目会独立复核，可能产生额外调用。

## 选区范围

每次发送固定当前选区快照，后续改变选区或编辑单元格不改变已发送内容。支持不连续选区、合并单元格和公式的当前结果；公式以普通文本发送。只包含所选非空单元格，不读取选区外单元格、图片或文件路径。多轮对话保留此前发送过的同一工作簿选区，点击“新建对话”可清空。

单次问答最多 64 个不连续区域、1000 个非空单元格和 64 KB 正文；超过限制会提示缩小范围，不静默截断。尚不支持图片问答。模型回答需核对，尤其金额、型号、工艺参数。

## 构建

安装 Rust 1.95.0，在 `gridcraft/` 中运行 `cargo build --locked --release -p gridcraft`。本仓库保留 `vendor/egui` 对齐补丁、`vendor/egui-winit` 剪贴板补丁和独立 `crates/metafile`，不能省略。Cargo 获取其余锁定依赖。原生程序需要系统 curl；Windows 10/11 使用内置 curl.exe。

打包示例：

```sh
python3 scripts/package.py --platform macos-arm64 --binary /path/to/target/release/gridcraft --output dist
python3 scripts/package.py --platform win64 --binary /path/to/target/release/gridcraft.exe --output dist
```

GitHub Actions 可手动构建 Mac arm64 / Windows x64，推送 `v*-api.*` 标签则在两个构建成功后创建 Release，附带 ZIP、完整源码和 SHA256 校验文件；不会覆盖已有 Release。也可以用 `scripts/publish.sh` 将本地包发布到尚未发布过的标签。源码与第三方许可随仓库保留。本机交叉构建包与 GitHub 原生构建包可能字节不同，请使用对应下载位置的校验文件。

已通过完整 `cargo xtask ci`，并验证本机模拟 API 的连接和选区问答、面板放大/恢复/最小化以及拖边调整大小。没有使用真实服务商 Key 实连，也没有完成 Windows 实机桌面验证；发布说明会明确列出这些边界。

接口文档参考：[OpenAI Responses](https://developers.openai.com/api/reference/resources/responses/methods/create)、[Chat Completions](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create)、[MiniMax](https://platform.minimax.io/docs/api-reference/text-openai-api)、[DeepSeek](https://api-docs.deepseek.com/)、[Kimi](https://platform.moonshot.cn/docs/api/chat)、[小米 MiMo 首次调用](https://mimo.mi.com/docs/en-US/quick-start/summary/first-api-call)。

## 许可

GridCraft、craft-libs 及本次修改按各目录 MIT / Apache-2.0 许可发布。品牌标识遵循原项目说明。本版本保持上游归属和 NOTICE；请勿将它称为上游官方发行版。
