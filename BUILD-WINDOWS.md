# Windows 构建说明

## 支持范围

v0.12.12 已为 Windows 10/11 x64 构建做好源码配置：

- React 前端使用系统 WebView2。
- Rust 后端、SQLite、实时 ASR、翻译和总结逻辑与 macOS 共用。
- API Key 使用 Windows Credential Manager 保存。
- 默认生成 NSIS `.exe` 安装程序。

## 1. 安装依赖

1. 安装 Node.js LTS。
2. 安装 Rust，并选择默认的 MSVC 工具链。
3. 安装 Microsoft C++ Build Tools，勾选 `Desktop development with C++`。
4. 确认已安装 Microsoft Edge WebView2 Runtime。较新的 Windows 10/11 通常已经包含。

安装 Rust 后，在新的 PowerShell 窗口执行：

```powershell
rustup default stable-msvc
node --version
npm --version
rustc --version
```

## 2. 构建安装程序

进入源码目录后执行：

```powershell
npm ci
npm test
npm run tauri:build:windows
```

安装程序生成在：

```text
src-tauri\target\release\bundle\nsis\
```

## 3. 首次运行

1. 安装并启动 `NUS 课堂同传`。
2. 在 Windows 麦克风隐私设置中允许桌面应用访问麦克风。
3. 打开应用设置，选择 ASR、翻译和总结模型。
4. 保存对应区域的 API Key。
5. 使用阿里云新加坡服务时，建议填写 `llm-` 开头的业务空间 ID。
6. 每周开课前可导入老师提供的 PDF、PPTX 或 PowerPoint 97-2003 PPT Slides；PPT 在应用内本地解析，不需要额外安装 Microsoft Office 或 LibreOffice。Syllabus 直接粘贴在课程设置中。

课堂、Slides、AI 问答与引用都保存在本机数据库中。建议定期在“课堂历史”中创建完整备份；备份可以在 Windows 和 macOS 之间迁移。

## 注意事项

- Windows 与 macOS 的 API Key 分别保存在各自系统凭据库中，不会自动同步。
- 未签名的本地安装程序可能触发 Windows SmartScreen 提示。对外分发前应配置 Windows 代码签名证书。
- `.msi` 构建还需要 Windows VBSCRIPT 可选功能；本项目默认使用无需该功能的 NSIS `.exe`。
