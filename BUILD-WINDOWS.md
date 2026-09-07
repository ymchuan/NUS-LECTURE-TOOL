# Windows 构建说明

## 支持范围

v0.13.0 已为 Windows 10/11 x64 构建做好源码配置：

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
6. 每周开课前可导入老师提供的 PDF 或 PPTX Slides；Syllabus 直接粘贴在课程设置中。

课堂、Slides、AI 问答与引用都保存在本机数据库中。建议定期在“课堂历史”中创建完整备份；备份可以在 Windows 和 macOS 之间迁移。

## 开发运行与白屏排查

在项目目录执行 `npm run tauri:dev`，并保持启动终端运行。开发模式需要同时运行 Vite 前端服务（`http://127.0.0.1:1420`）和 Tauri；不要只双击 `src-tauri\target\debug` 中由开发命令生成的程序。正式安装包内置前端资源，不依赖这个开发服务。

如果窗口白屏，先确认该地址的首页和 `/src/main.tsx` 能返回内容，不能仅凭端口在监听或窗口“响应正常”判断启动成功。`vite.config.ts` 已排除 `src-tauri`、`target` 和 `target-codex-test` 的文件监听，并将依赖扫描入口限定为 `index.html`，避免前端服务遍历大量 Rust 编译产物。Rust 源码仍由 Tauri/Cargo 负责重新编译。

`hard linking files ... failed. copying files instead` 是增量缓存退回复制的警告；exFAT 不支持硬链接，不代表构建失败。`failed to garbage collect ... 拒绝访问` 表示旧缓存清理失败，需检查文件占用和磁盘状态，不应直接清空项目或以管理员权限掩盖问题。

需要避开 exFAT 缓存时，可在 PowerShell 中把本次会话的 Cargo 输出改到健康的 NTFS 盘，例如：

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA 'nus-live-lecture-assistant\cargo-target'
npm run tauri:dev
```

首次切换会重新编译，不会删除原缓存或课堂数据。若磁盘报告健康警告，应先备份项目，再安排磁盘检查。

## 注意事项

- 使用可选 Ollama 应急补翻时，另行安装并启动 Ollama，执行 `ollama pull qwen2.5:3b` 后在语段翻译图标的补翻弹窗选择“Ollama 本地”。默认地址 `http://127.0.0.1:11434`，无需 Key；权重不包含在安装包中。“Ollama 云端”使用同一弹窗内独立配置的云模型和系统凭据库 Key，不要求本机安装模型。Ollama 不参与全局实时翻译、总结或问答。
- 定稿语段右上角可重试翻译，或使用芯片按钮本地补翻；历史课堂补翻结果会单独保存。长整课总结暂时优先用云端，详见 README 的本地模型限制。

- Windows 与 macOS 的 API Key 分别保存在各自系统凭据库中，不会自动同步。
- 未签名的本地安装程序可能触发 Windows SmartScreen 提示。对外分发前应配置 Windows 代码签名证书。
- `.msi` 构建还需要 Windows VBSCRIPT 可选功能；本项目默认使用无需该功能的 NSIS `.exe`。
