# GitPrism

跨平台 Git 工作树更改查看器，专为 review AI 编码代理产生的改动而做。
打开一个 git 仓库即可看到所有未提交更改（HEAD vs 工作树），逐文件查看
diff，并支持整体还原 / 单文件还原 / 单个修改块（hunk）还原。

技术栈：Tauri 2 + Vue 3 + TypeScript + Monaco Diff Editor，Rust 后端直接
调用系统 `git` CLI。

## 功能

- **多项目**：单窗口可同时打开多个仓库（工具栏项目芯片切换，各自的
  文件树 / Tab / 历史状态完全独立），也可「⧉ 新窗口」并排查看；
  支持把文件夹**拖入窗口**直接打开
- 打开本地 git 仓库，侧栏两个视图可切换：
  - **更改**：只列更改文件（默认展开），徽标：修改 (M) / 新增 (A) /
    删除 (D) / 重命名 (R) / 未跟踪 (U)
  - **全部文件**：项目完整文件树（遵循 .gitignore，默认折叠），更改过的
    文件带徽标，未更改的点开为只读内容预览
  - 两个视图都是目录树，可折叠、单子目录链自动合并成 `src/components/` 形式
- **自动检测更改**：Rust 侧 notify 文件监听（400ms 防抖，过滤 .git 内部
  噪音），外部改动自动刷新；工具栏的手动刷新按钮作为兜底
- Monaco 双栏（或单栏 inline）diff：语法高亮（按 Monaco 扩展名注册表自动
  识别主流语言）、词级差异、未变区折叠
- 还原：
  - hunk 还原走 git 原生交互——修改块旁的 **gutter ↶ 箭头图标**（VS Code
    风格），点击确认后反向 patch 经 stdin 交给 `git apply -R`；每次还原后
    全量刷新，hunk 偏移永远基于当前文件状态
  - 还原单个文件（文件行上 ↶；未跟踪文件为 ✕ 删除），按文件类型分别走
    restore / rm --cached / 删除
  - 还原全部 = `git reset --hard HEAD` + `git clean -fd`（不带 `-x`，
    .gitignore 命中的文件不受影响）
  - 所有还原操作先弹确认框
- 代码导航（Eclipse 快捷键 + IDEA 交互形态；git grep 文本索引，跨语言）：
  - `Ctrl+Shift+R` 打开资源（顶部浮窗文件模糊搜索）
  - `Ctrl+H` 全局搜索 / `Ctrl+Shift+G` 查找引用 → 结果进**底部停靠
    面板**（IDEA Find 工具窗样式，非模态，点结果跳转后面板保持）
  - `F3` / **Ctrl+点击** 符号跳转：唯一结果直接跳，多个候选在光标处弹
    **内联选择列表**（IDEA Choose Declaration 样式）
  - **Ctrl+悬停**符号变下划线链接；`Ctrl+L` 转到行、`Ctrl+F` 编辑器内查找
  - 语义级类型跳转需 LSP，不在本工具范围
- 15 套代码主题（VS / GitHub / Monokai / Solarized / Dracula / Nord 等
  浅深色），应用外壳颜色随主题联动；主题与最近打开仓库持久化
- 比较语义固定为 **HEAD vs 工作树**：AI 代理是否 stage 过都能看全

## 安装

从 [Releases](https://github.com/iccyuan/ai-diff/releases) 下载对应平台
安装包：Windows 用 `.exe`（NSIS 用户级一键安装，自动建开始菜单/桌面快捷
方式，无需管理员权限）；macOS 用 `.dmg`；Linux 用 `.deb` / `.AppImage`。

## 发布与自动更新

推送 `v*` 标签即触发 GitHub Actions 三平台构建并自动发布 Release：

```bash
git tag v0.1.0 && git push origin v0.1.0
```

应用启动时（及设置里「检查更新」）会比对 GitHub 最新 Release 自助升级。
要让签名校验通过，需在仓库 Settings → Secrets 配置：

- `TAURI_SIGNING_PRIVATE_KEY`：`tauri signer generate` 生成的私钥内容
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：私钥密码（无密码则留空）

公钥已写入 `src-tauri/tauri.conf.json`；CI 会用私钥签名并产出 `latest.json`。

## 开发

前置：Node ≥ 20、Rust（stable，Windows 用 MSVC 工具链）、git ≥ 2.23。

- Linux 另需：`libwebkit2gtk-4.1-dev build-essential libssl-dev`
  `libayatana-appindicator3-dev librsvg2-dev`
- macOS 另需：Xcode Command Line Tools
- Windows：VS C++ Build Tools + Windows SDK（WebView2 Win11 自带）

```bash
npm install
npm run tauri dev          # 开发运行
npm run tauri build        # 打包安装包（产物在 src-tauri/target/release/bundle/）
```

开发调试技巧：`AI_DIFF_OPEN_REPO=<仓库路径> npm run tauri dev` 启动后自动
打开指定仓库（Rust 侧读取环境变量，正式构建同样可用）。

## 测试

Rust 后端带完整测试（diff/status 解析器单测 + 真实临时仓库集成测试，
覆盖全部还原路径、hunk 偏移漂移、autocrlf=false/true/input 三种行尾配置、
空仓库、ignored 文件保护）：

```bash
cd src-tauri && cargo test
```

## 结构

```
src/                    Vue 3 前端
  monaco/setup.ts       Monaco worker、主题注册、扩展名→语言映射
  monaco/themes/        主题 JSON（vendored from monaco-themes, MIT）
  stores/repo.ts        仓库状态；"每次还原后必 refresh" 的一致性规则在这里
  stores/settings.ts    主题/布局/最近仓库（tauri-plugin-store 持久化）
  components/DiffView.vue   Monaco DiffEditor 生命周期 + hunk 还原按钮注入
src-tauri/src/git.rs    全部 git 逻辑：六个 tauri command + 解析器 + 测试
src-tauri/src/watcher.rs  notify 文件监听 + 防抖，emit "repo-changed" 事件
```

## 设计要点 / 已知行为

- git 调用永不经过 shell（参数向量 + `--` 分隔 + `-z` 输出），Windows 下
  带 `CREATE_NO_WINDOW`，无黑窗闪烁
- 非 UTF-8 文件按二进制处理（不显示文本 diff，仍可文件级还原）；
  单侧 > 5MB 不渲染 diff
- hunk 还原后若文件已与 HEAD 一致，会执行 `git checkout HEAD -- <file>`
  重写该文件：恢复 autocrlf 对应的本机行尾，并刷新索引里的行尾转换状态
  （否则 `git status` 会出现内容一致却报 M 的幻影条目）
- 空仓库（无提交）可打开：全部显示为新增/未跟踪，「还原全部」禁用
