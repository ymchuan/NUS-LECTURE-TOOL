# 双人协作开发

## 分支约定

- `main`：双方审查后接受的共同基线。当前首次源码 PR 合入前，main 仍只有 README。
- `zhengyi`、`ymc`：两人的现有开发分支。
- 后续优先从最新 main 创建短期功能分支，例如 `zhengyi/retry-translation`，每个 PR 聚焦一个功能或修复。

现有两个分支从完整源码各自导入，Git 共同祖先没有应用源码，因此首次整合冲突较多。不要直接用一个分支的目录覆盖另一个分支，也不要为了减少冲突而强制推送或重写对方历史。具体差异见 [分支对比](docs/branch-comparison-2026-09-08.md)。

## 核心概念

| 名称 | 在这个项目中的含义 |
| --- | --- |
| Git / GitHub | Git 在本机管理代码历史；GitHub 托管远程仓库并提供审查与协作界面 |
| working tree / stage | 工作文件 / 下一次准备提交的文件；保存文件不等于提交 |
| commit | 一次有说明的代码快照；commit 成功也不代表已上传 |
| branch | 指向一条开发历史的可移动名称；切换分支会改变本地文件内容 |
| origin | 这个本地仓库为 GitHub 远程地址起的名字 |
| fetch | 获取远程最新提交和分支信息，不把它们直接合入当前代码 |
| pull | 获取远程更新并将其整合到当前分支；使用 `--ff-only` 可在需要额外合并时停下 |
| push | 上传本地提交到指定远程分支；不会因此把该分支合入 main |
| Pull Request（PR） | 请求把来源分支的改动合入目标分支；例如 head=zhengyi、base=main |
| Draft PR | 尚在准备的草稿 PR，可查看差异、讨论和检查；转为 ready 后再审查合并 |
| merge / conflict | 合并两条历史 / Git 无法自动决定如何组合代码时，需要人处理 |
| rebase | 把提交重放到新起点，会改变提交标识；不要随意用于双方共享且已推送的历史 |

PR 与 `git pull` 不同：前者是 GitHub 上的审查和合并请求，后者是本机更新命令。PR 打开后，继续向其来源分支 push，PR 会自动更新。关闭 PR 不会自动删除分支；合并 PR 才会更新目标分支。

## 日常流程

先检查工作区是否有未提交内容，再获取远程变化：

```bash
git status
git fetch origin
```

首次源码 PR 合入 main 后，开始一个新功能：

```bash
git switch main
git pull --ff-only origin main
git switch -c zhengyi/retry-translation
```

完成修改后，检查具体文件和差异，按改动范围测试：

```bash
git diff
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

使用 `git add <实际改动文件>` 暂存审查过的文件，再提交和推送。例如在只修改 README 时：

```bash
git add README.md
git commit -m "docs: clarify setup instructions"
git push -u origin HEAD
```

在 GitHub 创建 PR，确认 base 为 main、head 为当前功能分支。描述改动目的、使用行为、测试结果及尚未完成的事项，请另一位开发者审查。确认后再合并。

开发过程中要吸收 main 更新，先保证工作区干净：

```bash
git fetch origin
git merge origin/main
```

若有冲突，逐文件理解并编辑冲突段，测试后用 `git add <已解决文件>` 和 `git commit` 完成；决定放弃本次未完成的合并时可使用 `git merge --abort`。不要使用 `git reset --hard` 或 `push --force` 作为常规解冲突办法。

## 提交内容与审查

提交源码、测试、图标、依赖锁文件及必要文档。不提交 API Key、课堂数据库、个人 Slides、模型权重、node_modules、编译缓存和大型安装包。模型由各自电脑安装；上传应用源码不会上传已下载的本地模型。

保持 PR 小而清晰；修改同一核心模块前先协调。PR 页面出现绿色可合并，只表示没有文本合并冲突，并不能证明功能兼容或测试通过。GitHub 自动测试与分支保护必须另行配置，创建 PR 本身不会自动启用这些功能。
