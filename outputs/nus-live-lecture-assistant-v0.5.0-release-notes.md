# NUS 课堂同传 v0.5.0

## 新增

- 实时英文转写可在 OpenAI 与阿里云 Paraformer 之间切换。
- 接入阿里云 `paraformer-realtime-v2` 双工 WebSocket 协议。
- 麦克风音频在本机转换为 16 kHz 单声道 PCM16，无原始音频落盘。
- 阿里增量句子、最终句子和错误事件接入现有转写与翻译界面。
- 暂停时结束阿里会话，继续时自动重连，减少无效计费。
- 阿里北京 ASR 与新加坡翻译/总结使用独立 Keychain 密钥。
- 设置页显示阿里实时转写官方参考成本约 CNY 0.864/小时。

## 当前模型链路

- 实时英文转写：OpenAI `gpt-live-transcribe` 或阿里 `paraformer-realtime-v2`
- OpenAI 翻译：`gpt-5.4-nano`
- OpenAI 总结：`gpt-5.4-mini` 或 `gpt-5.4-nano`
- 阿里翻译与总结：新加坡区 `qwen3.5-flash`

## 使用阿里实时转写

1. 打开“课程与术语”。
2. 在“实时英文转写”选择“阿里云 Paraformer”。
3. 填写阿里云北京区域 API Key。
4. 独立选择翻译与总结供应商并保存课程。
5. 开始听课；原始音频仅实时发送，不写入本地数据库。

## 验证

- 浏览器端 PCM 转换、转写段落和总结逻辑单元测试通过。
- Rust WebSocket 协议载荷与服务端事件解析单元测试通过。
- TypeScript、Vite、Rust 和 macOS release 构建通过。
- 桌面与 390px 窄屏设置界面检查通过，浏览器控制台无错误。

自动化环境未配置使用者的阿里云 API Key，因此没有执行会产生账户费用的真实云端调用。
