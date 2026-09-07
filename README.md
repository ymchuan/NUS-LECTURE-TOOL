# NUS 课堂同传

Windows 与 macOS 桌面应用源码，用于课堂英文实时转写、简体中文翻译和智能学习总结。当前版本为 v0.13.0。

## 已完成

- Tauri 2 + React + TypeScript 桌面应用。
- 麦克风 WebRTC 实时转写链路。
- 英文增量结果与最终语段处理。
- 基于 Responses API 的流式中文翻译。
- API Key 存入 macOS Keychain 或 Windows Credential Manager。
- 课程名称、上下文和专业术语提示。
- 双栏课堂界面、暂停/结束状态和音量显示。
- 无 API Key 可运行的演示课堂。
- macOS `.app`、Windows NSIS `.exe` 构建和自定义应用图标。
- SQLite 课程、术语、课堂、语段与阶段总结存储。
- 课程创建、编辑和快速切换。
- 可启停、可设优先级的中英术语表与别名。
- 课堂内容防抖自动保存，结束时完整落库。
- 异常退出后恢复未结束课堂，并可继续录制。
- 阶段总结面向课堂中的 1-2 分钟快速回顾，聚焦刚讲完的小主题和难点解释。
- 整课总结整合全部阶段知识、课堂依据和全程转写时间轴，生成可独立复习的详细资料。
- 阶段总结和整课总结可分别选择模型，并使用独立的结构上限与输出空间。
- 自动主题边界检测，也可随时手动总结。自动阶段总结不是逐句触发：当前阶段至少 5 个稳定语段，并在跨度达到 4 分钟且至少 10 段、出现主题转折且跨度达到 2 分钟、或跨度达到 8 分钟时触发；关闭“自动识别主题边界”即可只手动总结。
- 阶段总结仅在阅读位置处于底部时自动跟随；上滑回顾期间保留原位，并提示新增总结数量。
- 知识点、课堂依据、定义、例子、复习问题和考试提示归纳。
- Markdown 导出保留完整知识体系、例子、提示、问题和思维导图。
- 可视化思维导图，支持桌面与窄屏布局。
- 可选 Web Search 相关拓展；每项会讲清原理与课堂关联，并提供关键结论、公式变量、适用条件、操作步骤、数值算例和逐项可点击权威来源。
- 实时翻译、阶段总结、整课总结和问答继续使用 OpenAI 或阿里云百炼；Ollama 仅作为逐条语段的应急补翻来源，不出现在全局文本供应商设置中。
- Ollama 补翻使用原生 `/api/chat`：本地默认 `http://127.0.0.1:11434`，无需 Key；云端固定 `https://ollama.com`，使用独立的系统凭据库 Key，不复用 OpenAI Key。
- 当前和历史课堂的定稿语段均可点击翻译图标，在“当前云端配置 / Ollama 本地 / Ollama 云端”之间选择。选择仅作用于本次请求，失败保留原译文；历史补翻仅更新对应语段的译文和状态。
- 重试保留英文及旧译文，采用原语段的前一句作为上下文；独立请求 ID 隔离迟到响应。云端翻译有 55 秒、本地翻译有 110 秒总超时（含排队、重试和读取）。
- 当前推荐实时转写使用 Deepgram Nova-3；Deepgram 走实时 WebSocket，英文课堂音频不经过 OpenAI。
- Deepgram 实时转写同时显示 interim 草稿，但只有服务端 `speech_final` 才触发中文翻译；`is_final` 只表示识别片段稳定，不会单独触发翻译。当前课堂停顿阈值为 `endpointing=800ms`，额外的 `utterance_end_ms=1200ms` 用于结束判断。
- OpenAI 文本功能支持为实时翻译、阶段总结、整课总结分别配置模型和 API Key；旧版单一 OpenAI Key 仍可作为兼容回退。
- OpenAI 默认成本策略：实时翻译使用 `gpt-5.6-luna`，阶段总结使用 `gpt-5.6-luna` 或 `gpt-5.6-terra`，整课总结使用 `gpt-5.6-terra`。实际可用模型以 OpenAI 兼容平台的模型列表为准。
- 阿里云默认使用分层模型：`qwen-mt-lite` 实时翻译、`qwen3.7-flash` 阶段总结、`qwen3.7-plus` 整课总结。
- 实时翻译可切换到 `qwen-mt-flash` 术语质量档或 `qwen3.7-flash` 高并发低成本档。
- 整课总结可切换到低成本的 `qwen3.7-flash` 或高质量的 `qwen3.8-max`。
- v0.6.x 的 `qwen3.5-flash` 配置会自动迁移一次，自定义模型选择会保留。
- 实时英文转写可选择 OpenAI `gpt-live-transcribe`、阿里云新加坡 `qwen3-asr-flash-realtime-2026-02-10`、`qwen-audio-3.0-asr-flash-streaming` 或北京 `paraformer-realtime-v2`。
- Qwen3 ASR 使用明确的 `language: en`、800 ms 平衡断句和课件上下文，优先改善重口音英语被误判为中文、自然停顿与复杂教室环境。
- 应用不会按文字脚本静默丢弃识别结果；残留的语种误判会保留在课堂记录中，便于回看和继续诊断。
- Qwen Audio 3 会把每门课已启用的专业词表作为带权重即时热词和课程上下文发送；高、中、低优先级分别使用强、中、弱提示。
- CEG5104 Cellular Networks 内置课程推荐词表，覆盖 Singtel、StarHub、本地运营商、蜂窝网络架构缩写、无线接入、话务工程和排队论术语；空词表首次打开时自动加入，也可在设置中补充合并。
- CEG5301 Machine Learning with Applications 内置机器学习推荐词表，覆盖感知机、突触权重、诱导局部场、监督/强化学习、反向传播和常见模型术语。
- 设置中可指定课堂麦克风，便于选择外接定向麦克风，减少邻座谈话和环境噪声进入识别。
- 阿里低延迟模式按约 85 ms 采集、100 ms 级别发送 16 kHz 单声道 PCM，并即时显示非最终转写。
- 阿里低延迟模式复用新加坡翻译/总结密钥；北京 Paraformer 密钥继续独立保存在系统凭据库。
- 实时记录区显示“首字”延迟，绿色代表首个英文增量结果在 1 秒内返回。
- 翻译请求最多并行两条；临时 DNS、TLS、超时或连接失败会以递增间隔重试四次，并强制建立新连接。
- 可填写阿里云业务空间 ID，让转写、翻译和总结使用官方推荐的专属新加坡域名。
- 阶段总结和整课总结遇到临时连接失败时同样会重建连接并递增重试四次。
- 结构化总结若因输出截断或多消息响应而解析失败，会优先读取最后一个完整消息，并自动进行一次无联网精简重试，保证课堂核心总结可用。
- 实时记录仅在阅读位置处于底部时自动跟随；向上回顾后显示未读数量和“回到最新”。
- 标记会保存对应课堂时间、语段和可编辑备注，可从标记条跳转或删除。
- 同一门课可保存多周课堂，并按日期重新打开完整转写、阶段总结和整课总结。
- 历史课堂可由用户主动重新生成整课复习资料；应用不会在后台自动产生额外模型费用。
- 开始课堂前可导入 PDF 或 PPTX Slides；原始标题短语和技术缩写会参与 ASR 热词增强，页级文本用于当前页匹配和 AI 检索。
- Syllabus 以课程级纯文本保存，每周课堂共同使用。
- AI 问答支持本节课或整门课程范围，并可选择联网核验；课堂、Slides、Syllabus 和网页出处均可点击查看。
- AI 回答使用 Markdown 富文本排版，并以数学字体渲染行内和独立公式；旧回答中的转义符号也会在显示时自动修复。
- 自动匹配 Slides 的详情窗口直接还原 PDF/PPTX 原始页面，保留公式、图表和版式；提取文字仅在原页无法读取时作为备用内容。
- 历史课堂可导出 Markdown，并可创建或恢复包含数据库与 Slides 的完整备份。
- 历史列表显示课堂起止时间、时长和各类内容数量，并支持二次确认后永久删除单节课堂。

## 后续增强

- PDF/DOCX 笔记导出。
- 使用不同教室、麦克风和网络条件进行更长时间的真实课堂回归测试。

## 本地运行

```bash
npm install
npm run tauri:dev
```

只查看界面和演示模式：

```bash
npm run dev
```

浏览器访问 `http://127.0.0.1:1420/`。

## 构建 macOS 应用

```bash
npm run tauri:build
```

产物位于：

```text
src-tauri/target/release/bundle/macos/NUS 课堂同传.app
```

## 构建 Windows 应用

在 Windows 10/11 上安装 Node.js LTS、Rust MSVC、Microsoft C++ Build Tools 和 WebView2，然后在 PowerShell 中运行：

```powershell
npm ci
npm test
npm run tauri:build:windows
```

NSIS 安装程序位于：

```text
src-tauri\target\release\bundle\nsis\
```

完整准备步骤见 [BUILD-WINDOWS.md](BUILD-WINDOWS.md)。

## 使用实时课堂

1. 打开设置。
2. 填写课程名称、Syllabus 与专业术语。
3. 在“实时英文转写”中推荐选择 Deepgram Nova-3；它只需要 Deepgram API Key。
4. 如果改用阿里云，Qwen3 ASR 与 Qwen Audio 3 使用新加坡区域 API Key；仅选择 Paraformer 时才需要北京区域 API Key。
5. 在“翻译与总结模型”中选择 OpenAI 或阿里云百炼。OpenAI 下可分别填写实时翻译、阶段总结、整课总结的 Key 和模型；Ollama 配置只在语段补翻入口提供。
6. 如果使用第三方 OpenAI 兼容平台，在 OpenAI 配置区域填写完整的 `OpenAI Base URL`，例如 `https://example.com/v1`；留空则使用官方 `https://api.openai.com/v1`。该地址用于文本翻译、总结和问答，OpenAI 实时语音转写仍固定使用官方 Realtime 接口。
7. 建议在百炼“业务空间管理”中复制 `llm-` 开头的业务空间 ID，填入阿里云 API Key 上方的输入框。
8. 点击“开始听课”，填写本周课堂标题并按需导入老师提供的 PDF/PPTX Slides。
9. 首次使用时允许系统麦克风权限。

### 使用 Ollama 应急补翻

先启动 Ollama 并安装文本模型：

```powershell
ollama pull qwen2.5:3b
ollama list
```

在桌面应用的当前或历史课堂，点击语段右上角的翻译图标，选择“Ollama 本地”。本地服务默认 `http://127.0.0.1:11434`，模型默认 `qwen2.5:3b`。修改地址或模型后先点击“保存配置”，再点击“翻译此语段”。“当前云端配置”则复用现有实时翻译供应商、模型和翻译 Key。

选择“Ollama 云端”时，填写账户可用的云端模型 ID 和新建的 Ollama Cloud API Key，再保存配置。Key 仅存系统凭据库，不写入浏览器配置或备份；云请求固定发送到 `https://ollama.com/api/chat`，不依赖本机 Ollama 进程。模型 ID 请以 [Ollama 官方云端 API](https://docs.ollama.com/cloud) 为准，不要把本地转发用的 `-cloud` 标签直接照搬到云端 API。云端使用账户额度，可能受限流或套餐限制。

所有补翻选择都不会改变后续自动翻译、阶段总结、整课总结、问答或 Deepgram 转写，也不会在失败后自动转到另一家付费服务。本地请求关闭思考、保温 5 分钟，使用 16384 token 上下文和 4096 token 生成上限；补翻输入保守限制为 11000 UTF-8 字节（含提示词、术语和前一句），超限明确报错。云端总超时 55 秒、本地 110 秒，前端另有 65/120 秒保护。

8 GB 独立显存也可自行 `ollama pull qwen2.5:7b`，然后仅修改补翻弹窗中的本地模型，对比相同语段的速度与质量。本机 RTX 5050 8 GB 的 3B 小样本接口测试：预热后不同句子完整翻译约 0.33-0.54 秒；重新加载约 3.7 秒，安装后的首次初始化约 45 秒。这不包含 ASR、断句和界面延迟，不能保证课堂端到端 0.8-1 秒。上课前可先补翻一条历史语段预热。

兼容迁移：已使用 OpenAI/阿里云的模型配置保持不变。旧版全局 Ollama 配置迁移为逐句补翻，保留独立本地模型，并回到 OpenAI 默认文本模型、关闭自动阶段总结；请在开始新课堂前确认云端模型和 Key。

结束课堂时，应用会让你选择“结束并生成整课总结”或“结束并保存，不生成”。不生成整课总结不会影响实时转写、阶段总结和课堂记录；之后仍可从历史课堂中手动重新生成。

第三方地址必须支持 OpenAI Responses API（至少 `/v1/responses`，并按 OpenAI 兼容格式返回响应）；部分只实现 Chat Completions 的网关可能无法直接用于总结和问答。

## 推荐配置

重口音课堂推荐配置：

```text
实时英文转写  Deepgram nova-3
实时翻译      gpt-5.6-luna
阶段总结      gpt-5.6-luna / gpt-5.6-terra
整课总结      gpt-5.6-terra
```

成本策略：实时翻译请求最多，优先使用低延迟的 Luna；阶段总结通常使用 Luna，需要更强解释时切换 Terra；整课总结输入最长但调用次数少，使用 Terra 更稳，并在不需要时选择“结束并保存，不生成”。如果兼容平台没有这些模型名，请以其模型列表中的低成本/平衡模型替换。

Qwen3 ASR 新加坡公开价约为 0.00066 元/秒，即约 2.38 元/小时；更看重即时热词或较低成本时，可切回 `qwen-audio-3.0-asr-flash-streaming`。

截至 2026-08-20，阿里云新加坡区公开价中，`qwen-mt-lite` 约为输入 0.881 元/百万 Token、输出 2.642 元/百万 Token；`qwen3.7-flash` 在 32K Token 内约为输入 0.225 元/百万 Token、输出 0.974 元/百万 Token；`qwen3.7-plus` 在 256K Token 内公开价约为输入 2.998 元/百万 Token、输出 11.991 元/百万 Token。价格和活动折扣以控制台为准。

长期 API Key 只由 Rust 后端读取并按供应商及功能槽位分别保存在系统凭据库。前端不会收到长期密钥。旧版单一 `openai-api-key` 会在对应新槽位为空时回退使用。

课堂数据库保存在 macOS 应用数据目录中，默认不保存原始音频。录制过程中，语段、翻译和阶段总结会自动写入 SQLite；若应用意外退出，下次启动会显示恢复提示。

## 验证命令

```bash
npm test
npm run build
cd src-tauri && cargo test
```

## 官方接口参考

- [Realtime transcription](https://developers.openai.com/api/docs/guides/realtime-transcription)
- [Realtime WebRTC](https://developers.openai.com/api/docs/guides/realtime-webrtc)
- [Streaming Responses](https://developers.openai.com/api/docs/guides/streaming-responses)
- [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs)
- [Web Search](https://developers.openai.com/api/docs/guides/tools-web-search)
- [阿里云 Qwen Audio 3 实时语音识别](https://help.aliyun.com/zh/model-studio/qwen-audio-3-0-asr-flash-streaming)
- [阿里云 Qwen3 ASR Flash Realtime](https://help.aliyun.com/zh/model-studio/qwen3-asr-flash-realtime)
- [阿里云 Qwen3 ASR 客户端事件](https://help.aliyun.com/en/model-studio/qwen-asr-realtime-client-events)
- [阿里云 Qwen3 ASR 服务端事件](https://help.aliyun.com/zh/model-studio/qwen-asr-realtime-server-events)
- [阿里云实时语音识别模型选型](https://help.aliyun.com/zh/model-studio/asr-model)
- [阿里云文本生成模型选型](https://help.aliyun.com/zh/model-studio/text-generation-model)
- [阿里云 Qwen-MT 翻译模型](https://help.aliyun.com/zh/model-studio/machine-translation/)
- [阿里云 Qwen MT Lite](https://help.aliyun.com/zh/model-studio/qwen-mt-lite)
- [阿里云新加坡地域及接入域名](https://help.aliyun.com/zh/model-studio/singapore-regional-access-information)
- [Tauri Windows 开发环境要求](https://v2.tauri.app/start/prerequisites/)
- [Tauri Windows 安装程序](https://v2.tauri.app/distribute/windows-installer/)
- [阿里云 Qwen 3.7 Flash](https://help.aliyun.com/zh/model-studio/qwen3-7-flash)
- [阿里云 Qwen 3.8 Max](https://help.aliyun.com/zh/model-studio/qwen3-8-max)
- [阿里云 Fun-ASR 客户端事件](https://help.aliyun.com/en/model-studio/fun-asr-client-events)
- [阿里云 Fun-ASR 服务端事件](https://help.aliyun.com/zh/model-studio/fun-asr-server-events)
- [阿里云 Paraformer WebSocket API](https://help.aliyun.com/zh/model-studio/websocket-for-paraformer-real-time-service)
