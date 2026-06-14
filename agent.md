# Agent 指南

这个文件用于给工程代理提供足够的项目上下文，让它能在本仓库里安全地工作。

## 项目概览

Deep Index Board 是一个桌面剪贴板历史与本地语义搜索应用。

技术栈：

- 前端：React 19、TypeScript、Vite、原生 CSS。
- 桌面壳与后端：Tauri 2、Rust。
- 数据层：SQLite、FTS5、`sqlite-vec`。
- 本地 AI：macOS 上使用 CLIP/CoreML，后续规划 Windows/Linux 后端。

核心用户流程：

- 捕获剪贴板里的文字、图片、文件和文件夹。
- 将剪贴板历史保存在本地。
- 支持关键词搜索和基于图片向量的语义搜索。
- 点击历史项后，将内容粘贴回当前聚焦的应用。
- 用户数据、OCR、向量和模型推理都应保持在本地。

## 重要路径

- `src/`：React 前端。
- `src/App.tsx`：顶层双栏应用布局。
- `src/components/HistoryList.tsx`：历史加载、搜索模式、列表渲染和点击粘贴行为。
- `src/components/PreviewArea.tsx`：当前选中或悬停条目的预览区域。
- `src/components/StatusBar.tsx`：运行状态、内存和推理资源控制。
- `src-tauri/src/lib.rs`：Tauri builder、插件、应用初始化、快捷键、托盘、后台任务和命令注册。
- `src-tauri/src/commands.rs`：前端通过 Tauri 调用的命令 API。
- `src-tauri/src/dbm.rs`：SQLite 连接、schema 初始化、历史查询和向量存储。
- `src-tauri/src/schema.sql`：数据库结构。
- `src-tauri/src/tasks/`：OCR、文件提取、翻译、CLIP embedding 等后台重任务。
- `src-tauri/src/inference/`：推理后端抽象与 CoreML 后端。
- `src-tauri/resources/models/`：大型本地模型资源。除非任务明确要求处理模型打包，否则视为只读。

## 常用命令

Node 包管理器使用 pnpm。

```bash
pnpm install
pnpm build
pnpm tauri dev
pnpm tauri build
```

只检查 Rust/Tauri 后端时，在 `src-tauri` 目录运行：

```bash
cargo check
cargo test
```

Tauri 开发服务器配置在 `src-tauri/tauri.conf.json`：

- 前端开发地址：`http://localhost:1420`
- `beforeDevCommand`：`pnpm dev`
- `beforeBuildCommand`：`pnpm build`
- 前端构建产物目录：`../dist`

## 工程规则

- 改动范围要尽量贴近当前任务，避免顺手重构无关模块。
- 优先沿用现有 React、Tauri、Rust 写法，不轻易引入新抽象。
- 新增或修改 Tauri 命令时，通常需要在 `src-tauri/src/commands.rs` 实现，在 `src-tauri/src/lib.rs` 注册，并在前端用 `@tauri-apps/api/core.invoke` 调用。
- 前后端数据结构要保持一致，尤其是 `src-tauri/src/dbm.rs` 和 `src/components/HistoryList.tsx` 里的 `HistoryItem`。
- 修改数据库结构时，同步更新 `src-tauri/src/schema.sql` 和 `src-tauri/src/dbm.rs` 里的相关查询。
- 未经明确产品决策，不要把剪贴板内容、文件、图片、embedding 或 OCR 文本发到远程服务。
- 注意平台差异：macOS 代码常涉及 CoreML、Vision、Objective-C bindings 和 Command+V；Windows 代码使用 Ctrl+V 与 `windows-sys`。
- OCR、embedding、文件提取等耗时任务不要阻塞 UI 或 Tauri command handler，优先使用现有 heavy task manager。
- 不要改写或移动 `src-tauri/resources/models/` 下的大模型文件，除非任务明确要求。

## 编码与中文

仓库内中文文件按 UTF-8 处理。Windows PowerShell 5.1 默认读取无 BOM UTF-8 时可能显示乱码；读取中文文件时优先显式指定 UTF-8，例如：

```powershell
Get-Content README.md -Encoding UTF8
```

如果修改中文文案或注释，保持 UTF-8 编码。不要因为终端显示乱码就判断源文件内容损坏。

## Git 与 LFS

仓库包含模型子模块和 LFS 资源。`git status` 可能因为模型对象权限或 LFS clean filter 失败而报错。遇到这种情况时，不要改写模型资源；说明 LFS 失败原因，并继续做文件级检查。

## 验证建议

前端或共享 TypeScript 改动后运行：

```bash
pnpm build
```

Rust/Tauri 后端改动后，在 `src-tauri` 目录运行：

```bash
cargo check
```

只有需要交互验证时再运行：

```bash
pnpm tauri dev
```
