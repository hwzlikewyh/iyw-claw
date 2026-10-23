# 星河内核 0.164.0 升级记录

日期：2026-10-23。

## 目标与范围

内置星河从 0.163.0 更新到官方最新稳定版 0.164.0，继续使用进程内 App Server
和现有 ACP harness。本次更新应用源码；已安装程序需要构建发布后才会使用新内核。

验收标准：

1. 官方 release、标签对象、源码提交、生产依赖和运行时元数据一致。
2. 保留宿主认证与环境、MCP 已发布连接、命令描述、托管图片和压缩预算。
3. 依赖解析、源码语法、模块与资源引用、协议导出和迁移文件一致性检查通过。
4. 保留已有未提交工作，不新增或修改测试，不在本机编译、打包或启动桌面端。

执行步骤及验证：核对官方稳定版身份 → 三方迁移与冲突审查 → 更新依赖锁及元数据
→ 静态检查。以上源码步骤已完成，完整编译与产品验收待允许的 CI 环境执行。

## 官方来源

- 发布：https://github.com/openai/codex/releases/tag/rust-v0.164.0
- 发布日期：2026-10-23 04:48:10 UTC。
- tag object：`fe3b29455488c2e189e43ad5526a107900c5e1b7`。
- source commit：`6aaac694803004a63c6f393d153c96d22521a6b9`。
- 完成前再次查询 latest release，仍为 0.164.0；不使用预发布版或移动分支。

## 迁移决策

- 对旧官方源码、当前工作区和新官方源码做三方迁移，同步 22 个生产覆盖包。
- 保留认证隔离、宿主环境投影和网络策略；配置重载同时保留上游新增的 MCP
  企业认证取消上下文。进程内启动采用上游的项目上下文传递和关闭队列修复。
- MCP 会话状态查询继续读取同一已发布代际，避免建立第二套发现连接；适配新的
  认证参数、工具目录及元数据、能力过滤接口，并按实际连接失败情况报告认证状态。
- 保留可选命令描述、稳定及实验协议导出、托管图片回读和新版环境选择接口。
- 保留模型请求重试上限、HTTP 413 恢复、压缩请求预算、压缩后图片引用、Windows
  隐藏窗口、同 EXE helper 和 Win7 兼容路径。
- 继续保留反馈及插件包的 `tar = 0.4.46` 兼容覆盖。
- 上游改用 `mxc-sdk`，旧 `appcontainer_common` 和 `learning_mode_windows`
  覆盖不再进入依赖图；其历史源码保留。上游退出模块图的六个旧文件也保留原位。
- 0.1.304 的签名后校验暴露了新 MXC 的 `IsApiSetImplemented` 静态导入。
  生产入口已有的 `windows-link` 0.2/0.1 覆盖现对这一入口动态加载：仅从系统目录
  查找可选 DLL，API 缺失返回不支持，调用后释放句柄并恢复线程错误模式。
  x64、x86、ARM64 及 Win7 均共用该实现；不引入整份 MXC 源码覆盖、不改依赖锁。
  Windows 远端编译完成即执行现有 PE/运行时校验，签名后继续保留同一门禁。
- 未打开运行数据库或执行迁移；77 个迁移 SQL 的文件集合与内容均与官方源码一致。

## 已执行验证

- `cargo metadata --manifest-path src-tauri/Cargo.toml --format-version 1
  --filter-platform x86_64-pc-windows-msvc --locked --offline` 通过。
  Windows 解析图包含 120 个内核包，均为 0.164.0，其中 98 个锁定官方提交、
  22 个本地覆盖，无同名内核包的重复版本。跨目标 Cargo.lock 共包含 121 个内核包。
- `harness/codex/scripts/sync-upstream.ps1 -CheckOnly` 通过，标签对象及源码提交一致。
- `node src-tauri/scripts/prepare-xinghe-worker.mjs` 通过，生成 0.164.0 进程内元数据。
- 499 个生产 Rust 文件经已安装的 stable 1.99.1 rustfmt 语法解析，无错误。
- 22 个覆盖包无缺失生产模块，34 处内嵌资源引用完整；命令 item 构造保留 description。
- 稳定协议 20 处、实验协议 24 处命令 JSON 定义及 TypeScript 均保留可选 description。
- 对发生参数或字段变化的上游接口进行静态复核，修复 MCP 目录、能力过滤和认证
  参数适配；保留的旧文件不属于当前模块图。
- 本次范围的差异空白检查通过。

## 验证边界与后续验收

遵循仓库 `AGENTS.md` 和 `AGENT.md`：未新增或修改测试，未运行测试，也未在本机
执行桌面端 cargo check、编译、打包或启动。升级阶段未发布或替换已安装应用；
源码提交与合并按用户随后明确的交付指令执行。

依赖解析和语法检查不等同于类型检查、链接或真实账号验证。CI 仍需检查桌面和服务端
构建，再验收会话新建与恢复、模型请求、MCP、取消、压缩、图片回读及权限流程。
Linux/macOS 原生构建、Windows/Win7 实机和安装包尚未验证。

依赖解析仍提示未使用 crossterm 及上述两个旧 MXC patch；它们不进入当前解析图。

升级经验：三方文本合并无冲突仍可能遗漏本地扩展调用的参数、返回值或新增字段；
升级时应结合官方接口变化检查所有本地扩展。剔除测试代码后还应检查结构体字段和
初始化项的语法边界。

临时源码、备份和静态检查脚本位于
`C:\Users\iyw\AppData\Local\Temp\xinghe-upgrade-0.164.0-20261023`。
已验证目标路径后尝试清理，自动审批策略仍以 `blocked by policy` 拒绝该操作，
因此目录保留；这些文件未加入仓库或应用资源。

## 主分支交付

按用户后续要求提交、推送并合并到远程主分支。GitHub 的 `main` 比 GitLab 多一个
`v0.1.302` 发布提交，因此以包含两端历史的 `1de2a84a` 为基线创建独立工作树。
通过升级前备份、升级结果与最新主分支三方迁移，只交付本次升级的 475 个文件，
不提交原工作区的其他未提交改动。

唯一文本冲突位于 `unified_exec/write_stdin.rs` 的导入，保留主分支图片输出使用的
`ToolCallSource`，移除新版不再使用的 `Feature` 导入。主分支结果重新通过锁定离线
依赖解析、501 个生产 Rust 文件语法检查、模块与资源检查及 `git diff --check`。
上游标签和运行时元数据也已重新核对；编译和真实会话验证仍遵循上述 CI 边界。

## 宿主认证返回值适配修复

2026-10-23 的 macOS ARM64 发布构建暴露了本地 `host_auth.rs` 的 E0053：
0.164.0 的 `AuthProvider::add_auth_headers` 已返回 `Result<(), AuthError>`，
宿主扩展仍使用旧的无返回值签名。该扩展在所有平台编译，不是 macOS 专属问题。

修复将宿主方法与固定官方源码的 trait 对齐，通过 `?` 传递 Bearer 认证错误，
成功时返回 `Ok(())`。原有宿主 `token` 请求头、敏感标记及解析条件保持不变。
同包其他认证实现和 exec-server 认证实现均已使用新签名，无需修改。

本次只修改认证扩展及本段记录，不改动依赖、锁文件、CI 或发布版本。

修复后 `cargo check --manifest-path src-tauri/Cargo.toml -p codex-model-provider
--lib --locked --offline --jobs 2` 通过，完成真实依赖图下的类型检查；
`rustfmt +stable --edition 2024 --check` 与 `git diff --check` 通过。
依仓库约定未运行测试套件。macOS 的实际编译与签名由用户指定的 0.1.303
完整发布流程重新执行，不以 Windows 上的定向检查替代跨平台发布验证。

## MCP 状态类型引用修复

随后 Windows 发布编译在 `codex-mcp/src/mcp/status.rs` 暴露 E0432/E0425。
本地状态快照扩展误用了 `codex_protocol::mcp::McpAuthStatus`；固定版本官方源码
定义和同包 `mcp/mod.rs` 均使用 `codex_protocol::protocol::McpAuthStatus`。
修复统一导入及函数返回类型，保持 OAuth/EMA 失败转为未登录的既有逻辑。

本次扩大验证到桌面默认 features 的完整库依赖链，包含 MCP、内核、App Server
及进程内 harness。此检查与发布构建继续遵循锁定版本，不改动依赖或锁文件，
不以单一认证包的检查代表完整桌面代码已通过编译。

完整检查同时发现历史图片提升路径的 `UploadRequest` 缺少新版 `tags` 字段。
补入 `HashMap::new()`，与固定上游 `image_preparation.rs` 的上传初始化保持一致，
不添加额外标签或更改图片上传、回退和去重策略。

检查还发现托管图片存储忽略了新版认证方法的 `Result`。认证失败时现在记录失败
原因并回退既有附件存储，不继续发送未完成认证的图片请求；与上游认证 trait
要求的失败语义一致，不新增请求或重试。

最终代码执行 `cargo check --manifest-path src-tauri/Cargo.toml --lib --locked
--offline --jobs 2` 通过，覆盖默认桌面 features、MCP、内核、App Server、进程内
harness 与应用主库，最后一轮耗时 4m54s。差异空白检查和修改文件语法检查通过，
锁文件保持不变。未新增或运行测试套件。此为本机 Windows 类型检查；macOS 原生
编译、release 链接、签名和安装产物仍由 0.1.303 的完整 CI 重建验证。
