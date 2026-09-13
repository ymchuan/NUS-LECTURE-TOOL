# 16GB M2 MacBook Air 本地课堂翻译模型调研与实测报告

日期：2026-09-07  
项目：NUS 课堂同传  
报告状态：已完成首轮本机部署、候选模型下载和可重复基准；长时间热稳定性与人工盲评仍待补充。

## 结论先行

本机最终选择 **`qwen3:4b-instruct-2507-q4_K_M` 作为默认本地实时翻译模型**。它是本轮三个已部署候选中速度最快、体积最小，并在 10 条技术课堂压力样例的术语、数字、公式、否定和只输出译文检查中全部通过的模型。

已部署的另外两个对照模型是 `translategemma:4b` 和 `qwen3.5:4b`。TranslateGemma 是有价值的翻译专用对照，但本轮否定保真只有 35/45；Qwen3.5 的数字、公式和否定检查全部通过，却比 Qwen3 exact tag 慢，并且必须显式关闭 reasoning。具体数据见 [本地模型基准对比](local-translation-benchmark-comparison-2026-09-07.md)。

这里的“最合适”是针对本项目的工程目标，而不是宣称在所有语言、领域和硬件上的通用 SOTA。当前结论适用于：英文课堂短句、简体中文、16 GB 无风扇 M2 Air、单模型单并发和 OpenAI-compatible 本地接口。

## 使用场景与约束

### 实际输入分布

- 当前应用数据库包含约 `12,408` 条非空英文课堂语段。
- 语段平均约 `56.8` 个字符，长度 `p50=38`、`p90=125`、`p95=173`；最长异常段约 `3,176` 个字符。
- 语段主要来自 CEG5104、CEG5101、CEG5302 和 CEG5301 等课程，不是整篇文档批量翻译。
- ASR 确认一个英文语段后立即翻译；请求还可以使用课程名、上一句和启用的专业术语表。
- 产品目标是稳定语段在约 `0.8–2.0 秒`返回，整体体验目标约为 `1.5–3.5 秒`。

模型必须同时满足：

1. 英译简体中文短句自然、稳定；
2. 不丢数字、单位、公式、否定、模态词和不确定性；
3. 遵守课程术语表，并能用上一句消歧；
4. 暖机后低延迟，长课时不因无风扇机身热降频明显退化；
5. 直接兼容当前应用的 `/v1/chat/completions`，或适配成本足够低；
6. 不把课堂文本悄悄回退发送到云端。

### 设备资源

这是 16 GB 统一内存而不是 16 GB 独占显存。macOS、Tauri/WebView、音频缓冲、SQLite 和文件缓存都要占用内存，因此 4B Q4 是较稳妥的实时常驻规模。7B 及更大模型不适合作为默认常驻项，尤其不应在无风扇机型上同时加载多个模型。

## 已完成的现场部署

- 通过 Homebrew 安装 Ollama `0.33.3`。
- Ollama 可执行文件：`/opt/homebrew/bin/ollama`。
- 由 Homebrew LaunchAgent `homebrew.mxcl.ollama` 持久运行，地址为 `http://127.0.0.1:11434`。
- 已下载并验证三个模型：

  | 模型 | Ollama ID | 约占磁盘 |
  |---|---|---:|
  | Qwen3 Instruct 2507 | `qwen3:4b-instruct-2507-q4_K_M` | 2.5 GB |
  | TranslateGemma 4B | `translategemma:4b` | 3.3 GB |
  | Qwen3.5 4B | `qwen3.5:4b` | 3.4 GB |

- 旧的 `deepseek-r1:8b` 缓存仍在，但它不是翻译候选，也没有列入选型。
- 应用本地翻译并发门为 1。手动启动脚本还可设置 `OLLAMA_NUM_PARALLEL=1`、`OLLAMA_MAX_LOADED_MODELS=1`、`OLLAMA_CONTEXT_LENGTH=2048`、`OLLAMA_KEEP_ALIVE=30m`；通过 Homebrew 服务启动时，只有已配置的服务环境变量会生效，脚本变量不会追溯修改现有服务。

## 候选模型为何这样选

### 已实测候选

| 候选 | 选择理由 | 本轮结论 |
|---|---|---|
| `qwen3:4b-instruct-2507-q4_K_M` | 体积最小、中文指令能力成熟、OpenAI-compatible 本地生态好 | **默认** |
| `translategemma:4b` | 专用翻译方向，适合作为质量和术语对照 | 保留为备选，不作默认 |
| `qwen3.5:4b` | 较新的通用模型，用于观察新模型的事实保真与延迟 | 保留为实验对照 |

### 暂不部署的候选

| 候选 | 可能价值 | 暂不纳入默认的原因 |
|---|---|---|
| NLLB-200 distilled 1.3B/600M | 小型确定性 NMT，可能节省内存 | 需要自建 CTranslate2 适配服务；术语和上下文控制弱；许可需单独审核 |
| MADLAD-400 3B MT | 多语专用翻译 | Apple Silicon 后端、量化制品和接口适配尚未验证 |
| Hunyuan-MT 7B | 可能提供质量上限 | 统一内存余量、长课热稳定性和可信量化制品风险较高 |
| TranslateGemma 12B/27B、14B 级通用模型 | 可能提高离线质量 | 会挤压系统和 WebView 内存，不符合本机实时默认目标 |

NLLB、MADLAD 和 Hunyuan-MT 仍可作为后续实验，但不应在缺少本机性能和许可验证时写成“已部署”或“最终 SOTA”。

## 基准方法

基准脚本为 `scripts/benchmark-local-translation.mjs`，只使用 Node.js 内置模块，限制请求目标为回环地址。每个模型使用同一套 10 条技术课堂样例、流式 `/v1/chat/completions`、温度 0、单并发和术语约束。

样例覆盖梯度与学习率、蜂窝网络 handover/RSRP、Little’s Law、价格弹性、jitter buffer、贝叶斯公式、UE/LTE 认证顺序、单位、百分比、公式和否定条件。

- Qwen3：5 轮暖机（50 次请求）。
- TranslateGemma：5 轮暖机（50 次请求）。
- Qwen3.5：3 轮暖机（30 次请求）。
- `cold` 是当前基准进程中的第一个请求；若 Ollama 在运行前已把模型留在内存中，它不等同于真实卸载后的冷启动。
- 表面保真检查是回归信号，不是人工语义质量评分，也不等同 BLEU、COMET 等指标。
- 本轮没有完成 45–60 分钟连续课堂、峰值内存、swap、功耗或热降频测量。

## 本机实测结果

### 暖机单并发

| 模型 | 成功率 | TTFT p50 / p95 | 完整延迟 p50 / p95 | 输出速度 p50 |
|---|---:|---:|---:|---:|
| `qwen3:4b-instruct-2507-q4_K_M` | 50/50（100%） | **286 / 408 ms** | **1,048 / 1,384 ms** | 24.8 tok/s |
| `translategemma:4b` | 50/50（100%） | 669 / 1,407 ms | 1,692 / 3,220 ms | 21.9 tok/s |
| `qwen3.5:4b` | 30/30（100%） | 820 / 957 ms | 1,663 / 2,199 ms | 20.3 tok/s |

### 首个请求（冷启动近似值）

| 模型 | 首字延迟 | 完整延迟 |
|---|---:|---:|
| `qwen3:4b-instruct-2507-q4_K_M` | 4,905 ms | 5,432 ms |
| `translategemma:4b` | 6,559 ms | 7,165 ms |
| `qwen3.5:4b` | 5,220 ms | 5,933 ms |

### 保真与输出纪律

| 模型 | 术语 | 数字 | 公式 | 否定 | 只输出译文 |
|---|---:|---:|---:|---:|---:|
| `qwen3:4b-instruct-2507-q4_K_M` | 110/110（100%） | 45/45（100%） | 15/15（100%） | 45/45（100%） | 50/50（100%） |
| `translategemma:4b` | 110/110（100%） | 45/45（100%） | 15/15（100%） | 35/45（77.8%） | 50/50（100%） |
| `qwen3.5:4b` | 63/66（95.5%） | 27/27（100%） | 9/9（100%） | 27/27（100%） | 30/30（100%） |

示例：

```text
The gradient does not converge when the learning rate exceeds 0.1.
→ 当学习率超过 0.1 时，梯度不会收敛。
```

Qwen3 精确 tag 还做了 `UE`、`LTE` 等保留词复测。加强提示词后，50 次暖机请求均保留这些缩写。TranslateGemma 的主要回归问题出现在否定表达，不能仅凭术语、数字和公式通过就判定它更适合课堂默认使用。

## Qwen3.5 的特殊配置

Qwen3.5 默认支持 thinking。正式应用请求和基准请求必须带：

```json
{"reasoning_effort":"none"}
```

不带该字段时，模型可能把推理放入 `message.reasoning`、让 `message.content` 为空，或因达到 token 上限而失败。应用已经在本地测试请求和正式翻译请求中固定该字段；这也是选择 Qwen3 exact tag 作为默认模型时更简单稳妥的原因之一。

## 部署与使用

### 服务

当前电脑可检查服务：

```bash
bash scripts/start-local-translation.sh --check
```

没有 Homebrew LaunchAgent 的环境可使用：

```bash
bash scripts/start-local-translation.sh
```

脚本只允许 `127.0.0.1:11434`，并以单并发、单模型、2048 上下文为目标配置。不要把 Ollama 绑定到 `0.0.0.0`、局域网地址或公网隧道。

### 应用设置

1. 设置 → 实时翻译 → 选择“本地模型”。
2. 端点填写 `http://127.0.0.1:11434/v1`。
3. 模型填写 `qwen3:4b-instruct-2507-q4_K_M`。
4. 点击“测试模型”，确认返回的是简体中文译文而不是解释或思考过程。

本地翻译不需要 API Key。若实时英文转写使用 Deepgram，音频仍会发往 Deepgram；本地模型只替代转写后的中文翻译。阶段总结、整课总结和 AI 问答仍按各自设置使用云端文本模型。

### 复测

```bash
node scripts/benchmark-local-translation.mjs \
  --endpoint http://127.0.0.1:11434/v1 \
  --model qwen3:4b-instruct-2507-q4_K_M \
  --runs 5 --mode stream --concurrency 1 \
  --output outputs/qwen3-4b-instruct-2507-local-benchmark.json
```

把模型名和输出文件替换为 `translategemma:4b` 或 `qwen3.5:4b` 可复测另外两个候选。

## 后续验证计划

在把本地模型作为长期课堂默认前，仍建议：

1. 从真实课堂按课程和长度分层抽取 100–200 条，去除隐私后做人工盲评；
2. 测量 45–60 分钟连续请求的 p95、内存压力、swap 和温度/降频迹象；
3. 测试 ASR 残句、口语停顿、跨句消歧和连续术语变化；
4. 核对 Ollama 服务环境变量是否与手动启动脚本一致；
5. 若需要分发 NLLB/MADLAD/Hunyuan-MT，先完成模型许可证、量化文件和 Apple Silicon 运行时审查。

## 公开资料与许可提示

本轮联网核验了 Ollama 模型库中的三个实际 tag，以及 Ollama 的 OpenAI-compatible 接口说明：

- [TranslateGemma 4B（Ollama）](https://ollama.com/library/translategemma:4b)
- [Qwen3 4B Instruct 2507（Ollama）](https://ollama.com/library/qwen3:4b-instruct-2507-q4_K_M)
- [Qwen3.5 4B（Ollama）](https://ollama.com/library/qwen3.5:4b)
- [Ollama OpenAI compatibility](https://docs.ollama.com/api/openai-compatibility)

Ollama 本地模型的再分发仍须遵守各模型自己的条款：TranslateGemma 使用 Gemma Terms of Use，Qwen3 和 Qwen3.5 的模型卡/权重条款以对应发布页为准。本文的本机部署只供当前应用使用，不构成对模型许可的法律意见。

## 原始证据

- [`outputs/qwen3-4b-instruct-2507-local-benchmark.json`](qwen3-4b-instruct-2507-local-benchmark.json)
- [`outputs/translategemma-4b-local-benchmark.json`](translategemma-4b-local-benchmark.json)
- [`outputs/qwen3.5-4b-local-benchmark.json`](qwen3.5-4b-local-benchmark.json)
- [`outputs/local-translation-benchmark-comparison-2026-09-07.md`](local-translation-benchmark-comparison-2026-09-07.md)
