# GridCraft API Edition 0.12.1 · 2026-10-27

基于 GridCraft v0.12.1 的第三方 API 定制版，并非 Storytold 官方发行版。

## 新功能

- 新增 GPT / OpenAI、MiniMax、DeepSeek、Kimi、小米 MiMo 和自定义兼容 API 配置。支持 Chat Completions、Responses、Bearer 和 api-key 认证；地址、模型及高级 JSON 可编辑。
- 新增右侧 AI 选区问答，支持拖边调宽、放大/缩小、最小化/恢复、多轮提问、停止和复制回答。
- 发送时固定框选内容，支持不连续区域、合并单元格、数值及公式当前结果。回答不会自动改写工作簿；切换工作簿会隔离对话。
- 原有选区转英文、全工作簿转英文、选区转中文改用当前 API。保留完整双语复核、技术值保护及副本输出。
- 保留此前中文界面、XLS 兼容、图片显示、图片复制粘贴及文字布局修复。

## 下载与使用

Windows 10/11 x64 下载 `win64.zip`，完整解压后运行 `GridCraft.exe`；Mac Apple 芯片下载 `macos-arm64.zip`，解压打开 `GridCraft.app`。源码 ZIP 包含相邻依赖和许可证。请使用同一发布位置的 SHA256 文件校验下载。

窗口右上角“API 设置”填写你自己的 Key 和模型，测试连接后，框选单元格并打开“AI 选区问答”。模型预设可修改，不保证账户具有相应权限。软件不附赠 Key，服务商可能按调用计费。

API Key 默认仅本次运行使用；勾选记住时使用 Mac 钥匙串 / Windows 凭据管理器，不明文写入 JSON。只有点击发送、测试连接或翻译才会调用接口。选区问答最多 64 个区域、1000 个非空单元格、64 KB；暂不支持图片问答。XLS 完整转换仍需另装官方 LibreOffice，不捆绑在包内。

## 验证与限制

完整 `cargo xtask ci` 的 10 个步骤通过（格式、严格 Clippy、2515 项测试、文档测试、资产归属、依赖分层、WASM、命令清单等）。后续补充的面板拖动/放大/最小化交互测试也通过，最新 AI 测试 8 项全通过，严格 Clippy 通过。

本机原生测试窗口已通过 localhost 模拟 API 连接和选区问答。Mac arm64 及 Windows x64 本机交叉编译成功，ZIP 通过 CRC 和 SHA256 校验；Windows PE 为 x86-64 GUI，依赖检查未发现需另带的 MinGW DLL。GitHub Release 软件包由对应工作流重新构建。

尚未使用真实付费服务商 Key 实连，未完成 Windows 实机桌面测试，也未实测系统密钥存储的写入读回。发布包不含用户表格、API Key、配置或聊天记录。模型结果可能出错，请核对金额、型号和工艺参数。

Mac 包为本地临时签名，未做 Apple 公证；Windows 包没有发布者签名。系统可能显示来源提示，请按单位的软件安全流程处理。
