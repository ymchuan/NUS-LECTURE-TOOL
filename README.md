# NUS 课堂同传

Windows 与 macOS 桌面应用源码，用于课堂英文实时转写、简体中文翻译和智能学习总结。当前版本为 v0.12.13。

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
- 自动主题边界检测，也可随时手动总结。
- 阶段总结仅在阅读位置处于底部时自动跟随；上滑回顾期间保留原位，并提示新增总结数量。
- 知识点、课堂依据、定义、例子、复习问题和考试提示归纳。
- Markdown 导出保留完整知识体系、例子、提示、问题和思维导图。
- 可视化思维导图，支持桌面与窄屏布局。
- 可选 Web Search 相关拓展；每项会讲清原理与课堂关联，并提供关键结论、公式变量、适用条件、操作步骤、数值算例和逐项可点击权威来源。
- 实时翻译可在 OpenAI、阿里云百炼和本地 Ollama 模型之间切换；阶段总结、整课总结与 AI 问答继续独立使用云端文本模型。
- 本地翻译已经在 16 GB M2 MacBook Air 上用 Ollama 完成部署与实测；当前推荐 `qwen3:4b-instruct-2507-q4_K_M`，并保留 TranslateGemma 4B 与 Qwen3.5 4B 作为可选对照。
- 阿里云默认使用分层模型：`qwen-mt-lite` 实时翻译、`qwen3.7-flash` 阶段总结、`qwen3.7-plus` 整课总结。
- 实时翻译可切换到 `qwen-mt-flash` 术语质量档或 `qwen3.7-flash` 高并发低成本档。
- 整课总结可切换到低成本的 `qwen3.7-flash` 或高质量的 `qwen3.8-max`。
- v0.6.x 的 `qwen3.5-flash` 配置会自动迁移一次，自定义模型选择会保留。
- 实时英文转写可选择 OpenAI `gpt-live-transcribe`、阿里云新加坡 `qwen3-asr-flash-realtime-2026-02-10`、`qwen-audio-3.0-asr-flash-streaming` 或北京 `paraformer-realtime-v2`。
- 实时英文转写新增 Deepgram `nova-3` 与 `nova-2`，使用 16 kHz 单声道 PCM 流式音频、英文识别与即时英文增量显示。
- Deepgram 使用独立 API Key，支持在设置中保存、替换和删除；密钥按供应商存入系统凭据库。
- Deepgram 只负责英文转写，中文翻译、阶段总结、整课总结和 AI 问答继续使用独立选择的 OpenAI 或阿里云文本模型。
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
- 开始课堂前可导入 PDF、PPTX 或 PowerPoint 97-2003 PPT Slides；旧版 PPT 在本机解析，无需安装 Office，也不会上传到转换服务。原始标题短语和技术缩写会参与 ASR 热词增强，提取内容用于当前页匹配和 AI 检索。
- Syllabus 以课程级纯文本保存，每周课堂共同使用。
- AI 问答支持本节课或整门课程范围，并可选择联网核验；课堂、Slides、Syllabus 和网页出处均可点击查看。
- AI 回答使用 Markdown 富文本排版，并以数学字体渲染行内和独立公式；旧回答中的转义符号也会在显示时自动修复。
- 自动匹配 Slides 的详情窗口会还原 PDF/PPTX 原始页面；PowerPoint 97-2003 PPT 使用逐页文字兼容预览。需要查看旧文件的原始图表、公式和版式时，建议由 PowerPoint 导出为 PDF 或另存为 PPTX 后导入。
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
3. 在“实时英文转写”中选择 OpenAI、阿里云或 Deepgram；选择 Deepgram 后默认使用 Nova-3，也可切换到 Nova-2。重口音课堂可测试“Qwen3 ASR · 固定英语”，专业热词优先可选择提供英语提示的“Qwen Audio 3”。
4. Qwen3 ASR 与 Qwen Audio 3 都使用阿里云新加坡区域 API Key；仅选择 Paraformer 时才需要北京区域 API Key。
5. 使用 Deepgram 时，在“Deepgram API Key · 实时转写”中填写自己的密钥并保存；该密钥不要求 `sk-` 前缀。保存后可点击“测试连接”，先确认密钥、额度和所选模型可用。
6. 在“实时翻译”中选择 OpenAI、阿里云百炼或本地模型；在“总结与问答”中独立选择云端文本模型。本地翻译不需要 API Key，Deepgram 密钥也不能替代总结模型密钥。阿里文本模型使用新加坡区域 API Key。
7. 使用阿里云时，建议在百炼“业务空间管理”中复制 `llm-` 开头的业务空间 ID，填入阿里云 API Key 上方的输入框。
8. 点击“开始听课”，填写本周课堂标题并按需导入老师提供的 PDF/PPTX/PPT Slides。
9. 首次使用时允许系统麦克风权限。

## Deepgram 配置说明

Deepgram 连接由 Rust 后端直接建立，使用 `wss://api.deepgram.com/v1/listen` 与 `Authorization: Token` 认证。应用按英语 `en`、`linear16`、16 kHz、单声道发送麦克风音频，无需安装示例中的 ffmpeg、websocat 或 jq。

应用启用临时转写结果和 400 ms 静音断句，将多个 `is_final` 片段合并，收到 `speech_final` 或 `UtteranceEnd` 后提交完整语段进行翻译。暂停和结束课堂时会请求收尾，静音时发送 KeepAlive，以减少尾句遗漏和空闲断开。Nova-3 会使用已启用的课程热词作为 `keyterm` 提示。

Deepgram 设置与现有转写供应商、文本供应商分别保存；升级不会自动更改已有的 OpenAI 或阿里云模型选择。课堂音频会发送到 Deepgram，英文转写随后发送到已选文本供应商用于翻译和总结。额度、费用和网络可用性以 Deepgram 账户与实际连接结果为准。

“测试连接”会在内存中生成约 0.5 秒的静音 WAV，并将它发送到 Deepgram 的 HTTPS 预录转写接口。它用于验证已保存的 API Key、账户额度和所选 Nova 模型是否可以调用，可能产生一笔极小的 Deepgram 使用费用。测试成功不代表实时 WebSocket、麦克风采集、音频上行、KeepAlive 或课堂长连接已经验证；正式使用前仍应完成一次短时麦克风实时转写测试。

如果 Deepgram API Key 曾出现在聊天、截图、日志或其他公开位置，应立即在 Deepgram 控制台撤销旧密钥并生成新密钥。不要将新密钥写入源码、文档或再次发送到聊天中；只通过应用设置将它保存到系统 Keychain。

v0.12.13 的构建、测试和在线连通性记录见 [发布说明](outputs/nus-live-lecture-assistant-v0.12.13-release-notes.md)。

## 本地实时翻译（Ollama）

这条链路面向 16 GB Apple Silicon Mac。Ollama 只在本机常驻一个 4B 模型，产品并发门为 1、上下文窗口为 2048；对无风扇的 M2 MacBook Air，当前默认使用 `qwen3:4b-instruct-2507-q4_K_M`。

### 已部署模型与选型结果

当前开发机已经通过 Homebrew 安装 Ollama `0.33.3`，由 `homebrew.mxcl.ollama` LaunchAgent 持久运行在 `http://127.0.0.1:11434`。以下三个 4B 模型已经下载并通过同一套流式基准：

| Ollama 模型名 | 本机大小 | 定位 | 暖机 TTFT p50 | 暖机完整延迟 p50 | 术语/数字/公式/否定 |
| --- | ---: | --- | ---: | ---: | --- |
| `qwen3:4b-instruct-2507-q4_K_M` | 约 2.5 GB | **当前默认**，速度与保真度平衡最佳 | 286 ms | 1,048 ms | 100% / 100% / 100% / 100% |
| `translategemma:4b` | 约 3.3 GB | 翻译专用对照 | 669 ms | 1,692 ms | 100% / 100% / 100% / 78.3% |
| `qwen3.5:4b` | 约 3.4 GB | 较新的通用对照 | 820 ms | 1,663 ms | 95.5% / 100% / 100% / 100% |

上述结果来自 10 条技术课堂压力样例；Qwen3 运行 5 轮暖机请求（50 次），TranslateGemma 运行 5 轮（50 次），Qwen3.5 运行 3 轮（30 次）。它们都达到 100% 请求成功率。完整数据和方法见 [本地模型基准对比](outputs/local-translation-benchmark-comparison-2026-09-07.md) 与 [本地模型调研报告](outputs/local-translation-model-research-2026-09-07.md)，逐条结果保存在各模型的 JSON 文件中。

7B 及更大模型不作为 16 GB 无风扇机型的默认方案。这里的推荐是基于本机延迟和回归检查，不等同于通用翻译榜单上的“SOTA”；正式课堂仍建议用真实语段做人工抽查。

### 安装、启动与下载

已完成部署的电脑可以先做只读检查：

```bash
bash scripts/setup-local-translation.sh
```

首次安装或在另一台 Mac 重现部署时，明确选择要下载的模型：

```bash
bash scripts/setup-local-translation.sh --install --pull qwen3:4b-instruct-2507-q4_K_M
```

可选对照模型：

```bash
bash scripts/setup-local-translation.sh --install --pull translategemma:4b
bash scripts/setup-local-translation.sh --install --pull qwen3.5:4b
```

如果 Ollama 已经安装，去掉 `--install`。`--pull` 每次只接受一个明确的模型名；无参数运行时不会安装程序、启动服务或下载大文件。每次下载前会检查可用磁盘空间，已安装的 Ollama 和已存在的模型会自动跳过。模型只在首次下载时需要网络；下载完成后，本地翻译请求可在无外网时处理。

后续只启动或检查服务：

```bash
bash scripts/start-local-translation.sh
bash scripts/start-local-translation.sh --check
```

当前开发机使用 Homebrew LaunchAgent，因此 Mac 登录后会自动保持服务运行。手动启动脚本仍可用于没有 LaunchAgent 的环境；它会把 Ollama 限制在 `127.0.0.1:11434`，并设置 `OLLAMA_NUM_PARALLEL=1`、`OLLAMA_MAX_LOADED_MODELS=1`、`OLLAMA_CONTEXT_LENGTH=2048` 和 `q8_0` KV cache。若 Ollama 已由其它方式启动，这些变量不会追溯修改，需按该方式重启服务后才会生效。

### 应用设置

1. 打开设置，在“实时翻译”中选择“本地模型”。
2. 模型名填入 `qwen3:4b-instruct-2507-q4_K_M`；也可选择已下载的 `translategemma:4b` 或 `qwen3.5:4b` 做对照。
3. 本地端点保持 `http://127.0.0.1:11434/v1`。
4. 点击“测试模型”，确认能返回简体中文且不包含解释或思考过程，再开始课堂。

应用发送翻译时使用 `temperature: 0`，并固定发送 `reasoning_effort: "none"`。后一个字段对 Qwen3.5 尤其重要：不指定时，模型可能把推理放入响应而不是直接返回译文。本地模型只替代英文转写之后的实时中文翻译；阶段总结、整课总结和 AI 问答仍使用“总结与问答”中选择的 OpenAI 或阿里云模型。

### 本地基准测试

启动 Ollama 后，可对任一已下载模型运行内置技术课堂样例：

```bash
node scripts/benchmark-local-translation.mjs \
  --endpoint http://127.0.0.1:11434/v1 \
  --model qwen3:4b-instruct-2507-q4_K_M \
  --runs 5 \
  --mode stream \
  --concurrency 1 \
  --output outputs/qwen3-4b-instruct-2507-local-benchmark.json
```

将 `--model` 和输出文件名替换为 `translategemma:4b` 或 `qwen3.5:4b` 即可复测。脚本会记录本进程首个请求、暖机后 p50/p95、流式首字延迟、近似输出速度、失败率，以及译文的术语/数字/公式/否定保真检查。“首个请求”只有在模型事先未加载时才近似真实冷启动；表面字符检查只用于回归对比，不等于人工语义质量评分，也没有替代 45–60 分钟连续课堂稳定性测试。

### 安全与隐私边界

- 脚本和本地 Ollama 翻译不需要、不读取、也不写入任何 API Key；已有云端密钥仍保存在系统凭据库。
- 应用和脚本只接受回环地址上的本地翻译端点。不要将 Ollama 绑定到 `0.0.0.0`、局域网 IP 或通过公网隧道暴露 `11434` 端口。
- “本地翻译”不等于“整个课堂完全离线”。如果实时英文转写选择 Deepgram/OpenAI/阿里云，音频仍会发送到对应服务；云端总结和问答也会发送课堂文本。
- Deepgram Key 与本地模型是两条独立链路：本地翻译不需要 Deepgram Key，但选择 Deepgram 做英文转写时仍需在应用设置中保存自己的新 Key。

## 推荐配置

重口音课堂推荐配置：

```text
实时英文转写  qwen3-asr-flash-realtime-2026-02-10
实时翻译      qwen-mt-lite
阶段总结      qwen3.7-flash
整课总结      qwen3.7-plus
```

Qwen3 ASR 新加坡公开价约为 0.00066 元/秒，即约 2.38 元/小时；更看重即时热词或较低成本时，可切回 `qwen-audio-3.0-asr-flash-streaming`。

截至 2026-08-20，阿里云新加坡区公开价中，`qwen-mt-lite` 约为输入 0.881 元/百万 Token、输出 2.642 元/百万 Token；`qwen3.7-flash` 在 32K Token 内约为输入 0.225 元/百万 Token、输出 0.974 元/百万 Token；`qwen3.7-plus` 在 256K Token 内公开价约为输入 2.998 元/百万 Token、输出 11.991 元/百万 Token。价格和活动折扣以控制台为准。

长期 API Key 按供应商分别保存在系统凭据库；设置表单只在用户输入时暂存密钥并交给 Rust 后端保存，读取状态时只返回是否已保存，不会取回密钥内容。

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
- [Deepgram 实时流式识别](https://developers.deepgram.com/docs/getting-started-with-live-streaming-audio)
- [Deepgram 断句与临时结果](https://developers.deepgram.com/docs/understand-endpointing-interim-results)
- [Deepgram Keyterm Prompting](https://developers.deepgram.com/docs/keyterm)
- [Deepgram KeepAlive](https://developers.deepgram.com/docs/audio-keep-alive)
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
