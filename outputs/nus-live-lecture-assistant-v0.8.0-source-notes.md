# NUS 课堂同传 v0.8.0 跨平台源码

- Windows 使用 Credential Manager 保存 OpenAI 与阿里云 API Key。
- macOS 继续使用 Keychain。
- 新增 Windows NSIS `.exe` 构建命令与 `.ico` 图标配置。
- 麦克风、SQLite、实时 ASR、翻译、阶段总结和整课总结逻辑保持跨平台共用。
- Windows 安装程序需要在 Windows 10/11 环境中生成，步骤见 `BUILD-WINDOWS.md`。
