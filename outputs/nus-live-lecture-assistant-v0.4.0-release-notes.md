# NUS 课堂同传 v0.4.0

## 新增

- 翻译和总结可在 OpenAI 与阿里云百炼之间切换。
- 阿里云新加坡区 `qwen3.5-flash` 支持流式中文翻译。
- `qwen3.5-flash` 支持阶段总结、整课总结、知识点和思维导图。
- 阿里云 Responses API 联网搜索及来源提取。
- OpenAI 与阿里云 API Key 分别保存在 macOS Keychain。
- OpenAI 总结可进一步切换为低成本 `gpt-5.4-nano`。
- 联网搜索设置增加独立计费提示。

## 当前模型链路

- 实时英文转写：OpenAI `gpt-live-transcribe`
- OpenAI 翻译：`gpt-5.4-nano`
- OpenAI 总结：`gpt-5.4-mini` 或 `gpt-5.4-nano`
- 阿里翻译与总结：`qwen3.5-flash`

## 使用阿里云百炼

1. 打开“课程与术语”。
2. 在“翻译与总结模型”选择“阿里云百炼”。
3. 保留 OpenAI API Key 用于实时英文转写。
4. 填写阿里云百炼新加坡区 API Key。
5. 根据成本需要开启或关闭联网拓展，然后保存课程。

## 验证

- 前端单元测试：8 项通过。
- Rust 单元测试：6 项通过。
- TypeScript、Vite 和 macOS release 构建通过。
- 桌面与 390px 窄屏设置界面检查通过。
- 浏览器控制台无错误。

自动化环境未配置使用者的阿里云 API Key，因此没有执行会产生账户费用的真实云端调用。
