# 课堂同传模型供应商调研

调研日期：2026-08-17

## 结论

本版采用混合供应商架构：

- 实时英文转写可选择 OpenAI `gpt-live-transcribe` 或阿里云 `paraformer-realtime-v2`。
- 翻译和阶段/整课总结可选择 OpenAI 或阿里云百炼。
- 阿里云方案使用新加坡区 `qwen3.5-flash`。
- 联网拓展跟随所选文本供应商，但保持独立开关。

## 文本价格

以下均为官方按量原价，活动优惠和税费未计入：

| 模型 | 输入 / 百万 Token | 输出 / 百万 Token | 用途 |
| --- | ---: | ---: | --- |
| OpenAI `gpt-5.4-nano` | USD 0.20 | USD 1.25 | 实时翻译 |
| OpenAI `gpt-5.4-mini` | USD 0.75 | USD 4.50 | 默认总结 |
| 阿里新加坡 `qwen3.5-flash` | CNY 0.734 | CNY 2.936 | 翻译与总结 |

`qwen3.5-flash` 支持结构化输出和联网搜索，适合高频课堂文本处理。阿里云内置联网搜索还会产生搜索策略费用；新加坡区官方标价为 CNY 73.392381 / 1000 次，并叠加检索内容产生的模型输入 Token 费用。因此应用默认保留独立开关，成本敏感时可以关闭。

## 接口兼容性

阿里云百炼提供 OpenAI 兼容的 Responses API。应用固定使用新加坡通用端点：

`https://dashscope-intl.aliyuncs.com/compatible-mode/v1`

翻译使用流式 Chat Completions，阶段与整课总结使用 Responses API；联网时添加 `web_search` 工具，并从 `web_search_call.action.sources` 提取来源。模型生成的 JSON 还会经过本地解析和类型校验后再写入 SQLite。

## 阿里实时转写

`paraformer-realtime-v2` 通过北京区域 WebSocket 提供英文实时识别，官方按量价格为 CNY 0.00024/秒，约 CNY 0.864/小时。应用在本机将麦克风音频降采样为 16 kHz、单声道、PCM16 小端格式，然后按短音频块发送；增量句子直接更新右侧英文，最终句子触发所选文本模型翻译。暂停会结束当前会话，继续时创建新会话，避免暂停时间继续计费。

北京区 ASR 与新加坡区文本模型使用独立 API Key。两把密钥均只保存在 macOS Keychain，前端不会读取；原始音频不会落盘。真实课堂仍需针对教室距离、背景噪声与印度口音做 A/B 测试。

## 官方资料

- OpenAI GPT-5.4 nano：https://developers.openai.com/api/docs/models/gpt-5.4-nano
- 阿里云 qwen3.5-flash：https://help.aliyun.com/zh/model-studio/qwen3-5-flash
- 阿里云 Responses 兼容接口：https://help.aliyun.com/en/model-studio/compatibility-with-openai-responses-api
- 阿里云新加坡 Base URL：https://help.aliyun.com/en/model-studio/base-url
- 阿里云联网搜索：https://help.aliyun.com/zh/model-studio/web-search
- 阿里云语音识别模型：https://help.aliyun.com/zh/model-studio/asr-model/
- 阿里云 Paraformer WebSocket：https://help.aliyun.com/zh/model-studio/websocket-for-paraformer-real-time-service
- 阿里云客户端事件：https://help.aliyun.com/zh/model-studio/paraformer-client-events
- 阿里云服务端事件：https://help.aliyun.com/zh/model-studio/paraformer-server-events
