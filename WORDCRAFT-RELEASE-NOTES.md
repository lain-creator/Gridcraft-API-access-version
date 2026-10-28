# WordCraft API Edition · 自定义 API 中英翻译

> 2026-10-28：本说明保留作历史记录。WordCraft 已迁移至独立仓库 [WordCraft-API-Edition](https://github.com/lain-creator/WordCraft-API-Edition)，新下载入口为[独立 Releases](https://github.com/lain-creator/WordCraft-API-Edition/releases/latest)。按用户要求，在新 Mac / Win64 安装包校验后删除本仓库旧 WordCraft Release 和附件，保留源码标签和历史。后续不在 GridCraft 仓库混发 WordCraft。

版本 `0.11.0-wordcraft.20261027.2`，基于上游 v0.11.0 的定制版，非官方发行版。

- 新增“API 设置”：GPT / OpenAI、MiniMax、DeepSeek、Kimi、小米 MiMo、自定义 OpenAI 兼容接口。用户自行填写地址、模型 ID、API Key；支持 Chat Completions / Responses、Bearer / api-key、超时及附加 JSON。
- 移除固定网站调用，不回退旧服务。三个翻译按钮共用当前配置，点击后只发送范围内候选文字，不上传完整文件、图片或选区外正文。
- 密钥默认仅本次运行使用；可选 Mac 钥匙串 / Windows 凭据管理器，不写普通配置、文档或安装包。默认 HTTPS，不跟随重定向，远程 HTTP 需明确允许。
- 保留原文并生成独立 DOCX 副本；完整双语不重复追加，数字/型号本机校验。按批调用，拟跳过项复核一次；格式/网络错误不自动重试。停止等待不能撤销已发送的请求。
- 保留汉化、UTF-8 正反选区及表格取值修复；保留中文字体替代、DOCX 正/负裁剪和图片边缘拉伸修复。不捆绑微软字体。

Mac Apple 芯片下载 macos-arm64.zip；Windows 10/11 x64 下载 win64.zip，完整解压运行，需要 [Visual C++ x64 运行库](https://aka.ms/vc14/vc_redist.x64.exe)。附完整源码 ZIP 和 SHA256。

新版 Mac / Win64 包发布并校验成功后，仅删除旧固定网站版 v0.11.0-wordcraft.20261027.1 的 Release 和附件；保留源码标签与 Git 历史，GridCraft 历史发布不动。

本机模拟接口验证认证、两种协议、取消、错误和不泄露密钥；保留选区、字体及图片回归。连接测试可能产生少量费用，但不发送文档。未使用真实付费 Key 或业务文件，未做本次 Windows 桌面实机验证。复杂排版、SVG、EXIF 旋转仍可能有限制。Mac 临时签名、未公证；Windows 未做发布者签名。
