# 本地翻译模型基准对比

日期：2026-09-07  
项目：NUS 课堂同传  
硬件：16 GB Apple M2 MacBook Air（统一内存、无风扇）  
运行时：Ollama 0.33.3，`http://127.0.0.1:11434/v1`

## 结论

当前默认模型选为 **`qwen3:4b-instruct-2507-q4_K_M`**。它在本机暖机后的首字延迟和完整响应延迟都明显低于另外两个候选，并在本轮 10 条技术课堂样例的术语、数字、公式、否定和输出纪律检查中全部通过。`translategemma:4b` 是翻译专用的可用对照，但否定保真回归较弱；`qwen3.5:4b` 较新且保留了较好的事实保真，不过延迟更高，且必须显式关闭 reasoning。

这不是通用翻译榜单上的“SOTA”结论。它是针对本项目“英文课堂短句 → 简体中文、16 GB M2 Air、单并发、低延迟”的工程选型结论。

## 已部署模型

| 模型 | Ollama ID | 本机大小 | 状态 |
|---|---|---:|---|
| Qwen3 Instruct 2507 | `qwen3:4b-instruct-2507-q4_K_M` | 约 2.5 GB | 默认 |
| TranslateGemma | `translategemma:4b` | 约 3.3 GB | 已部署，可作翻译专用对照 |
| Qwen3.5 | `qwen3.5:4b` | 约 3.4 GB | 已部署，可作较新通用模型对照 |

Ollama 由 Homebrew 安装，当前服务名为 `homebrew.mxcl.ollama`，监听回环地址。应用默认只允许一个本地翻译请求并发，以避免 16 GB 统一内存设备上的排队和换页放大。旧缓存中的 `deepseek-r1:8b` 不是翻译候选，也没有纳入比较。

## 测试方法

基准脚本：`scripts/benchmark-local-translation.mjs`。每条请求使用同一套课堂技术样例、流式 OpenAI-compatible `/v1/chat/completions` 接口、温度 0、单并发和相同的术语约束。样例覆盖梯度/学习率、蜂窝网络切换、Little’s Law、价格弹性、抖动缓冲、贝叶斯公式、UE/LTE 认证顺序、数字、单位、公式和否定表达。

- Qwen3：5 轮暖机，每轮 10 条，共 50 条请求。
- TranslateGemma：5 轮暖机，每轮 10 条，共 50 条请求。
- Qwen3.5：3 轮暖机，每轮 10 条，共 30 条请求。
- “冷启动”是脚本进程中的第一个请求；只有运行时此前确实没有加载模型时，它才近似真实模型装载时间。
- 术语、数字、公式和否定检查是可重复的表面回归信号，不是人工语义评分；没有把它们冒充 BLEU、COMET 或人工盲评。
- 本轮没有完成 45–60 分钟连续课堂、峰值内存、swap、功耗或热降频测试；这些仍是发布前的后续验证项。

## 结果

### 暖机单并发

| 模型 | 成功率 | TTFT p50 / p95 | 完整延迟 p50 / p95 | 估算输出速度 p50 |
|---|---:|---:|---:|---:|
| `qwen3:4b-instruct-2507-q4_K_M` | 50/50（100%） | 286 / 408 ms | **1,048 / 1,384 ms** | 24.8 tok/s |
| `translategemma:4b` | 50/50（100%） | 669 / 1,407 ms | 1,692 / 3,220 ms | 21.9 tok/s |
| `qwen3.5:4b` | 30/30（100%） | 820 / 957 ms | 1,663 / 2,199 ms | 20.3 tok/s |

### 首个请求（冷启动近似值）

| 模型 | 首字延迟 | 完整延迟 |
|---|---:|---:|
| `qwen3:4b-instruct-2507-q4_K_M` | 4,905 ms | 5,432 ms |
| `translategemma:4b` | 6,559 ms | 7,165 ms |
| `qwen3.5:4b` | 5,220 ms | 5,933 ms |

### 关键保真检查

| 模型 | 术语 | 数字 | 公式 | 否定 | 只输出译文 |
|---|---:|---:|---:|---:|---:|
| `qwen3:4b-instruct-2507-q4_K_M` | 110/110（100%） | 45/45（100%） | 15/15（100%） | 45/45（100%） | 50/50（100%） |
| `translategemma:4b` | 110/110（100%） | 45/45（100%） | 15/15（100%） | 35/45（77.8%） | 50/50（100%） |
| `qwen3.5:4b` | 63/66（95.5%） | 27/27（100%） | 9/9（100%） | 27/27（100%） | 30/30（100%） |

一个直接请求示例：

```text
The gradient does not converge when the learning rate exceeds 0.1.
→ 当学习率超过 0.1 时，梯度不会收敛。
```

Qwen3 精确 tag 还额外复测了 `UE`、`LTE` 等术语；在加强“术语表中标记为保留英文的源词必须原样保留”提示后，50 次暖机请求均保留这些缩写。

## 运行时注意事项

Qwen3.5 默认可能产生 reasoning 内容。应用和基准请求必须携带：

```json
{"reasoning_effort":"none"}
```

不带该字段时，模型可能把推理放入 `message.reasoning`、令 `message.content` 为空，或在 token 上限内无法返回译文。该字段已写入应用的本地测试请求和正式翻译请求。

模型下载和推理均不需要 API Key。若英文转写选 Deepgram，音频仍会发往 Deepgram；本地模型只负责转写之后的中文翻译。Ollama 端点应继续只绑定 `127.0.0.1`，不要暴露到局域网或公网。

## 原始数据

- [`outputs/qwen3-4b-instruct-2507-local-benchmark.json`](qwen3-4b-instruct-2507-local-benchmark.json)
- [`outputs/translategemma-4b-local-benchmark.json`](translategemma-4b-local-benchmark.json)
- [`outputs/qwen3.5-4b-local-benchmark.json`](qwen3.5-4b-local-benchmark.json)

这些 JSON 保留逐条输入、译文、延迟、估算 token 数和检查结果，便于后续更换提示词、运行时或模型 tag 后复测。
