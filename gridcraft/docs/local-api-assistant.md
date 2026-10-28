# 第三方 API 与选区问答

2026-10-27，API 定制版基于 GridCraft v0.12.1，保留此前 XLS、图片及中文界面修复。完整使用、接口兼容和构建说明见仓库根目录 `../README.md`（从本项目根目录计算）。

新增 `ui.ai`、`ui.ai.settings`、`ui.ai.ask`、`ui.ai.status`、`ui.ai.selection`、`ui.ai.cancel`、`ui.ai.clear`、`ui.ai.minimize`、`ui.ai.close` 界面命令。问答在后台执行，只发送不可变选区快照；当前工作簿修改、选区切换、供应商切换均不改变已发请求。工作簿切换会取消本地等待并隔离对话。

`file.translateEnglish` / `file.translateChinese` 的程序调用需显式带 `allowApiUpload:true`，不再使用旧版 `allowHttpUpload`。翻译快照包含当前 API 配置，密钥只在内存中传给原生服务，不序列化到控制状态。打开文件不调用 API。

界面位于窗口右上角：“AI 对话”和“API 设置”。右侧面板支持调整宽度、放大、最小化、恢复和关闭。保留三个既有翻译按钮，改为调用当前 API。

本地版 `0.12.1-api.20261028.1` 将问答、图表及数据分析合为一个对话，由模型按问题判断并返回有界结构化建议，程序本机校验。支持自动判断横向记录和表头；少量数据或空图表方案保留分析。只有显式“生成并插入图表”创建图表和数值快照辅助表。使用范围及命令见 [local-ai-insights.md](local-ai-insights.md)。

本版本使用 OpenAI 官方文档核对 Responses 请求和返回结构，兼容接口保留 Chat Completions 选项。模型名、端点和附加参数可编辑。没有代替用户申请或附赠任何 API Key，也不将用户自己的文档上传到 GitHub。
