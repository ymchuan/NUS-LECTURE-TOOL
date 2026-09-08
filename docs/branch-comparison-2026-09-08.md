# zhengyi 与 ymc 分支对比

对比时间：2026-09-08。以下结论针对固定提交，后续分支更新需重新比较。

- `zhengyi`：`70c72283c7173596da0536a700043a576bfbae43`，应用版本 0.12.13。
- `ymc`：`78412eac46e2a80c3f6083b5cafd93f6f9498511`，应用版本 0.13.0。
- 共同祖先：`83542136572f0c501b89f0a61dcf85a512160739`，仅有 README。

## 结论

两边都是完整源码分别导入仓库，不能把版本号较高的 ymc 当作已经包含 zhengyi 全部功能的升级版。ymc 的导入提交没有保留此前逐次开发历史，因此 Git 无法证明它具体从哪个旧版源码开始；以下依据最终文件内容判断功能差异，不推断作者删除功能的意图。

从 zhengyi 看向 ymc，有 76 个路径不同：26 个同名文件内容不同，9 个仅 ymc 有，41 个仅 zhengyi 有。其中包含测试、文档、模型基准和生成文件，不能用增删行数评价工作量或质量。

使用 `git merge-tree --write-tree --name-only origin/zhengyi origin/ymc` 预演合并，得到 26 个冲突文件，主要是 add/add（双方分别加入同名文件），另有 README 内容冲突。预演没有切换分支或修改工作文件。

## 功能差异与整合建议

| 项目 | zhengyi | ymc | 建议 |
| --- | --- | --- | --- |
| 实时中文翻译 | 翻译供应商独立选择，支持整堂课自动本地翻译；默认本地模型为 Qwen3 4B Instruct 2507 | 全局翻译跟随云端文本供应商；Ollama 用于主动逐句补翻 | 保留独立实时翻译设置，再接入补翻能力 |
| 单句补翻 | 无独立的补翻弹窗与历史单行保存流程 | 当前/历史课堂可选当前云端、Ollama 本地或 Ollama 云端；失败保留旧译文 | 移植弹窗、请求隔离和单行保存，并适配双方配置字段 |
| 流式完整性 | 已有增量解析和错误处理 | 增加结束标志检查、截断检测、超时和独立请求 ID，防止旧响应污染重试 | 优先整合并保留对应测试 |
| Deepgram | Nova-3/Nova-2、400 ms 断句、课程热词、KeepAlive、UtteranceEnd/尾句处理和连接测试 | 固定 Nova-3、800 ms 断句；有最终片段合并，但没有同等热词、KeepAlive 和 UtteranceEnd 处理 | 以 zhengyi 的实现为基础逐项审查，不用整文件覆盖 |
| OpenAI 连接和密钥 | 官方地址与通用 OpenAI Key | 自定义 HTTPS Base URL；翻译、阶段总结、整课总结独立 Key，并回退兼容旧 Key | 作为可选配置整合，保留已有用户设置 |
| 默认云端模型 | gpt-5.4-nano / gpt-5.4-mini 等当前配置 | gpt-5.6-luna / gpt-5.6-terra，并迁移部分旧模型名 | 模型名是代码配置值，本次未核验所选 API 是否可调用；整合时不强制替换用户模型 |
| 结束课堂 | 当前流程生成整课总结 | 弹窗允许跳过，之后从历史生成 | 建议整合，给用户控制调用成本的选择 |
| Slides | PDF、PPTX、旧版 PPT 本地解析和预览 | PDF/PPTX；缺少旧版 PPT 解析组件与依赖 | 保留旧版 PPT；审查 CSP 的 blob/worker 配置，避免影响预览 |
| 本地模型部署 | 三模型实测报告、安装/启动/基准脚本；端点限定本机 | 默认 qwen2.5:3b 补翻；本地原生 Ollama API 与官方 Ollama Cloud | 两种用途可以共存；先明确“本机”和远程服务的界面边界 |
| 开发体验 | 常规 Vite 配置 | 排除 Rust 编译目录监听，限制依赖入口扫描 | 可作为小型独立改动优先合入 |
| 测试 | 含 Deepgram 设置、课堂录音会话和旧版 PPT 回归测试 | 新增补翻、单行保存、截断检测和总结边界测试，但缺少上述部分测试文件 | 合并测试集合，不以数量判断哪一边更好 |

关键证据文件：`src/lib/modelPreferences.ts`、`src/types.ts`、`src/hooks/useLectureSession.ts`、`src-tauri/src/deepgram_asr.rs`、`src-tauri/src/lib.rs`、`src-tauri/src/documents.rs`；ymc 独有的 `src/components/TranslationDialog.tsx` 和 `src/lib/translation.ts`；`src-tauri/src/storage.rs` 的 `save_transcript_translation`。

## 当前推荐协作操作

1. 建立 `zhengyi → main` 草稿 PR，审查当前可运行源码基线；它不表示已经整合 ymc。
2. 双方确认后，将基线 PR 合入 main。草稿本身不会修改 main。
3. 从更新后的 main 新建功能分支，分批整合 ymc 的功能，保留其提交来源与贡献者信息。
4. 建议顺序：Vite 监听修复 → 跳过整课总结 → 补翻与保存/完整性检查 → 独立 Key 与自定义地址。
5. 核心重叠文件需要逐项适配，不能一律选择“全部保留我的”或“全部保留对方的”。ymc 的首个源码提交包含多项功能，整条 cherry-pick 仍会产生大量冲突。
6. 整合后同时测试本地实时翻译、补翻、旧配置迁移、Deepgram 暂停/结束尾句和 PPT 导入，再提交面向 main 的 PR。

本轮仅做静态对比与合并预演；没有合并两个产品版本，也没有运行 ymc 的应用或调用付费 API。zhengyi 基线此前已在本机通过前端 66 项测试、Rust 64 项测试（2 项按需测试跳过）及前端构建；ymc 发布说明中的测试成绩属于对方记录，本次未独立复测。
