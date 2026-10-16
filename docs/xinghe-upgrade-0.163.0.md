# 星河内核 0.163.0 升级记录

日期：2026-10-16。

## 目标与范围

内置星河从 0.161.0 更新到官方稳定版 0.163.0，继续使用进程内 App Server
和既有 ACP harness。此改动更新应用源码；已安装程序需要后续构建发布才会使用新内核。

验收标准：

1. 官方 release、tag object、源码提交、生产依赖和运行时元数据一致。
2. 保留宿主认证、环境投影、MCP 已发布状态、命令描述、图片引用和回合归属。
3. 适配上游压缩接口，保留有界请求、HTTP 413 恢复及 Windows/Win7 兼容补丁。
4. 依赖图、生产源码语法、模块引用、协议导出和迁移文件一致性检查通过。

执行步骤及验证：核对稳定版身份 → 三方迁移生产源码与本地补丁 →
解析并锁定依赖 → 静态审查与一致性检查。以上源码步骤已完成，编译及产品验收待 CI。

## 官方来源

- 发布：https://github.com/openai/codex/releases/tag/rust-v0.163.0
- 发布日期：2026-10-15 21:30:38 UTC。
- tag object：`129b8ec7ee6f4511557a4856279f529025c25af4`。
- source commit：`83086188ae0614bd882818ff1aa23f3194962012`。
- 仅采用稳定 release，不使用 alpha 或移动分支。

## 迁移决策

- 对旧官方源码、本地生产覆盖源码和新官方源码做三方迁移，保留工作区已有改动。
- 同步 21 个既有生产覆盖包；增加反馈组件的生产覆盖包，共 22 个。
- 新反馈组件固定 `tar = 0.4.45`，与现有 Qdrant 的 `tar >= 0.4.46`
  无法共同解析。反馈组件仅调整该依赖为 0.4.46，其生产源码保持官方内容。
- MCP SDK 随上游升级到固定提交的 3.5.1；主应用自身的 1.3.0 SDK 保留既有边界。
- 适配压缩 replacement context、prepared prefix 和是否实际安装的返回值，
  保留输入预估与单次 HTTP 413 恢复，避免沿用已移除的 `InitialContextInjection`。
- 前台压缩的重试和模型回退共享 600 秒请求预算；后台压缩请求也使用该预算。
  成功后的历史提交不受请求预算中途取消，认证和付款错误不触发模型回退。
- 远程压缩在完整替换和后台前缀安装时保留可回读的托管图片 ID。
- 上游拆分 rollout 持久化策略后，旧格式仍保留带 description 的命令完成记录。
- 保留 Windows 同 EXE 沙箱角色、隐藏窗口和 Win7 动态 ConPTY 路径。
- 同步 SQLite migrations 0059–0061 和 thread-history migration 0008。
  六套目录的文件集合及全部 77 个 SQL 与官方源码一致，保持 LF；未打开运行库执行迁移。
- 上游移除的两个 zsh 历史源码文件保留在原位置，已退出模块图。
- 不增加 CLI 专属界面或改变宿主会话、队列、认证存储的所有权。

## 已执行验证

- `cargo metadata --manifest-path src-tauri/Cargo.toml --format-version 1
  --filter-platform x86_64-pc-windows-msvc --locked --offline` 通过。
  解析图包含 117 个内核包，版本均为 0.163.0，其中 95 个固定官方提交、22 个本地覆盖，
  无同名内核包的重复版本。Cargo.lock 包含其他目标在内的 118 个内核包，版本和提交一致。
- 上游同步脚本的只读校验通过：release tag 和 peeled commit 与锁文件一致。
- `node src-tauri/scripts/prepare-xinghe-worker.mjs` 通过，生成 0.163.0 的进程内元数据。
- 456 个更新生产 Rust 文件经 rustfmt 语法解析，无错误；生产模块文件引用完整。
- 稳定协议 20 处、实验协议 24 处命令 JSON 定义含可选 description，TypeScript 同步。
- 全部命令 item 构造保留 description；迁移文件集合及内容一致。
- 静态复核宿主 key 与策略认证隔离、配置重载环境、MCP 单代际查询、
  工具等待与截断、压缩预算、图片引用、迟到事件和 Win7 ConPTY 分支。
- 本次范围的 `git diff --check` 通过。

## 验证边界

按仓库 `AGENTS.md` 和 `AGENT.md`，未新增或修改测试文件，未运行测试，
未在本机执行桌面端 cargo check、编译、打包或启动，未发布或替换安装中的应用。

语法及依赖解析不证明类型检查、链接或真实账号行为。CI 仍须验证桌面和服务端编译，
随后验收会话新建/恢复、模型请求、MCP、命令取消、压缩、图片回读、权限和旧库升级；
Linux/macOS 原生构建及 Windows/Win7 实机行为尚未验证。
依赖解析保留了未使用 crossterm patch 的提示。

本次临时源码、备份和检查脚本位于
`C:\Users\iyw\AppData\Local\Temp\xinghe-upgrade-0.163.0-20261016`。
递归和逐项清理均被自动执行策略以 `blocked by policy` 拒绝，目录因此保留；
临时文件不在仓库内，未加入应用资源。

## 远程主分支集成

2026-10-16 按用户要求提交、推送并合并。升级增量在基于远程 main
`a4a4b5ecd5b8319fcfd1cb2ed75854425b792fd0` 的独立工作区中集成，
保留主分支的 MCP 输出预览、SQLite 校验预算及应用版本 0.1.285。
当前原工作分支的其他提交和工作区变更未纳入升级提交。

集成源码再次通过 locked/offline metadata、456 个 Rust 文件语法解析、
生产模块引用、34 处内嵌资源路径、77 个迁移 SQL、协议导出、运行时身份和
`git diff --check` 检查。编译、测试和产品验收仍遵循上述边界。
