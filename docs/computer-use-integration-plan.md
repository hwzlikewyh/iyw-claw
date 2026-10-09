# 原生 Computer Use 与压缩摘要实施计划

日期：2026-10-08。用户已确认使用 codeg 的原生方案、移除旧 open-computer-use，并增加压缩摘要查看。

设计依据：C:/Users/Obsidian/Codex/D-projects-iyw-iyw-claw/2026-10-08-computer-use-full-integration-design.md。
上游固定为 codeg v0.34.0（592131c72e2a05478e7f8398a91cb653c8e3996f），驱动 cua-driver 0.32.0。

## 步骤与验证

1. 已实施：执行核心、共享权限与 helper -> Windows 桌面/server/helper 检查、helper 构建及桌面主程序实际构建均通过。
2. 已实施：HTTP MCP、会话生命周期与取消 -> 18 个工具 schema/绑定一致；截图保留 MCP 图片；入队停止代次与发起 Agent 身份审查。
3. 已实施：设置、状态栏、共享选择及停止浮层 -> 最终 TypeScript、定向 ESLint 和 Next 静态构建通过，正式安装包仍需 CI。
4. 已实施：受管驱动、打包、旧实现迁移与 Fusion -> 环境引导程序编译、安装脚本语法、相关 Fusion test/vet/build 通过。
5. 已实施：摘要实时与历史贯通 -> 显式摘要标记、追加分片、Claude 边界/恢复、星河真实摘要传递；无摘要旧记录隐藏入口。
6. 进行中：交付验收 -> 本机静态检查及 Windows 只读链路已完成；代码尺寸约束、最终 CI 包、完整 UI 操作及 macOS/Linux 实机和签名包仍有待办。

## 边界

- 复用现有 HTTP MCP、Transport、EventEmitter、app_metadata 和 Fusion 分发契约。
- 不恢复独立 MCP 可执行文件；不更换星河 harness，不改数据库表。
- 授权只在内存，重启不恢复；全局停止不等待普通驱动队列。
- macOS Computer Use 要求 14.4+，不提高应用自身 13.5 的最低要求。
- 旧实现只清理应用所有的配置与组件，不删除用户自行安装的工具。
- Claw 按 AGENTS.md 不新增/运行自动化测试。用户随后明确允许本轮定向编译和构建，已完成本机检查；正式签名安装包仍交由 CI。
- 用户已授权通过 DBX 准备分发数据并继续上传发布；正式库驱动 0.32.0、六平台制品和
  0.1.260 五平台绑定已完成，线上下载校验通过。Fusion 部署和 Claw 应用正式发布未执行。

## 记录

- 正常入口与发布步骤见 `docs/computer-use.md`，源码来源与许可证见
  `src-tauri/licenses/codeg-computer/NOTICE.md`。
- Windows 验证使用官方固定归档与执行文件摘要，未输出窗口标题、截图、树或剪贴板内容。
- Claw 未新增/运行自动化测试；Fusion 保留并补充组件绑定/平台矩阵正式回归。
- 最后审查修复了摘要分片替换、停止后的旧队列、远端迟到响应、helper 版本错配、
  服务端安装包拒绝 helper、更新失败后服务不可用、Win7 缺失 API 等边界。

## 2026-10-08 收尾证据

本轮收尾顺序（用户要求继续）：

1. 阶段完成：整理本次新模块的职责与尺寸 -> 参数解析、helper 分发/动作/错误职责拆分；新增代码文件均 <=300 行，仍有长函数与多参数待整理。
2. 已完成静态审查：摘要、远程状态与打包失败路径 -> 修复摘要恢复只留末片、完成事件覆盖 metadata、本机往返切换的旧回调、驱动错误日志泄露输入、空 helper 包。
3. 已完成本机构建与静态验证：desktop/server/helper 检查、桌面主程序构建和 Next 静态构建通过；Fusion 驱动上传发布、五平台绑定与线上下载校验已完成。

本轮验收：既有 62 项工具契约不变；18 项电脑工具与共享权限仍闭环；摘要的实时与历史入口一致；宿主切换不采纳旧结果；没有越过本机构建与发布授权边界。

| 检查 | 结果与范围 |
| --- | --- |
| Windows 桌面 Rust 检查 | `cargo check --bin iyw-claw --locked --offline --jobs 4` 通过，有编译警告 |
| 服务端与 helper Rust 检查 | `server-runtime,computer-helper` 的所有 bin 检查通过，有编译警告 |
| 最终 helper 构建与身份 | debug helper 构建通过；版本 `0.1.256`、目标 x64 Windows、当前源码 SHA256 全部匹配 |
| Windows 驱动只读链路 | 固定归档及执行文件摘要匹配；protocol 14 握手、Configure、34 个窗口枚举、Halt 确认通过 |
| 前端检查 | 最终源码 TypeScript 检查与 Computer Use/摘要/Transport 定向 ESLint 通过，0 error / warning |
| 能力与配置 | 18 个 schema 与能力绑定一致；既有 62 个 schema 内容与格式保留；JSON/脚本语法检查通过 |
| Fusion 分发 | 相关正式回归、vet 和 build 通过；老客户端绑定兼容与新版可选驱动覆盖 |
| diff | `git diff --check` 通过 |

整理前 helper 源码指纹：`6ecd673595654ba5c1d53dc1d154a7db6879df13f52d0d7b1b34dd47354cf91e`。
原工作树收尾源码指纹：`5833d121b366f9ea8b4ee2ed38120f966ef6c321f9c7596e98ac35f70b978101`。
上述指纹已由原工作树最终 helper 构建与 `--identity` 校验确认。
最终前端修复按宿主隔离设置、权限、窗口列表、缩略图和共享面板的状态，
并阻止旧宿主权限请求随后在新宿主打开设置；这些修改之后已重新执行类型/lint。

尚未通过的门禁：

- 新增代码文件的 300 行限制已满足；静态盘点仍有 76 个函数超过 50 行（54 个 Rust、22 个 TS/TSX）、45 个 Rust 函数超过 3 个位置参数，嵌套与圈复杂度未完整验收。
  不能宣称代码规范全部满足；后续按职责继续整理，不以机械分割替代可维护性。
- 最终源码的正式 CI 安装包、签名、公证、安装/升级/回滚实机验证未执行。
- 未点击、输入、截图、读取可访问性树或剪贴板；Agent 到真实桌面的完整交互未验收。
- macOS/Linux 无本轮实机证据；现有 ad-hoc macOS 包禁用该功能。
- Fusion 新上传入口仍需部署编译白名单；当前驱动已通过本机发布逻辑和 DBX 正式事务
  上传发布，0.1.260 五平台线上分发验证通过。Claw 应用仍需同步新版本、构建正式包并验证安装。

本轮静态验收：392 个新 Rust 模块引用存在；18 个电脑 schema 与能力绑定一致；
原有 62 个 schema 数据保持一致；安装脚本、helper 发布脚本和 YAML/JSON 语法有效；
新 Rust 模块语法无错误。共享文件 `connection.rs` 的旧 tree-sitter 语法器对既有
async closure 有两处误报，与 HEAD 一致，本轮没有新增解析错误。静态语法检查不能替代编译。

## 2026-10-09 编译与 DBX 准备

- 用户允许本轮本机构建后，最新 desktop cargo check、server/helper 全 bin check、helper build、桌面主程序 `cargo build --bin iyw-claw` 和 `pnpm build` 均通过，有 Rust 编译警告。
- 当前 helper 身份与源码指纹一致。桌面主程序实际构建已通过；未启动应用或真实输入。
- Fusion 相关正式回归、vet/build 在新绑定下限调整为 0.1.257 后再次通过。
- DBX 迁移先测试后正式；新工具 ID `366614153281925120`，草稿 ID `366614153281925121`，
  `hidden / install_on_initialize=0 / draft / min_client_version=0.1.257`，测试库重跑 0 条变更。
- 五个归档覆盖六个平台，归档与内部执行文件摘要全部匹配。制品与证明在
  Fusion `artifacts/environment-components/cua-driver/0.32.0/`，发布步骤见该仓库的
  `docs/native-computer-driver-preparation.md`。

## 2026-10-09 上传发布与线上验证

- 官方五个归档完成六个平台项的 TOS PUT、Fusion 回读与归档检查；六个平台的工具制品
  已关联 `366614153281925121` 并发布 0.32.0，设置 stable 推荐策略。
- 正式库 0.1.257 至 0.1.259 已发布且绑定不可变，改用 0.1.260；驱动兼容下限同步为
  0.1.260，隐藏目录与按需安装策略保持有效。五平台新绑定 revision 166，包含可选 cua-driver。
- 五个平台通过线上 Fusion resolve/download 获取 TOS 地址，完整下载、大小和 SHA-256
  全部匹配。记录位于 Fusion 制品目录的 publication-index.json、binding-index.json、download-verification.json。
- 本次没有额外 Minisign 签名，沿既有固定摘要镜像路径发布；没有替换线上 Fusion，
  没有更新或发布 Claw 应用，没有执行真实桌面输入或跨平台实机操作。
- 新版本门槛对应的 Fusion 相关回归、vet/build 和 diff 检查通过；测试库同步 0.1.260
  兼容下限但继续保持草稿。临时发布进程已停止、仓库临时源码已清理；临时脚本、
  二进制和字节码删除被自动审批拒绝（blocked by policy），这些文件仍保留。

## 2026-10-09 主分支集成

用户已要求提交、推送并合并至主分支。Claw 本次改动单独迁移到最新 `origin/main`
集成，没有带入原功能分支的其他未合并提交；应用版本沿用主分支 `0.1.259`，
新功能的正式应用发布仍使用 `0.1.260` 或更新未占用版本。
Fusion 原生驱动分发已合并并推送到 `master`，合并后的相关回归、vet/build 通过。

集成工作树的 helper 源码指纹为 `4043d7ed39589b0f8b681ffab46767969430df60ebfb6eb7a43a8ecf5595354f`。
Git checkout 的换行字节会影响该指纹，正式 helper 必须从同一发布工作树重新构建并核验。
此前 `0.1.256` 的 debug helper 与构建记录保留为原工作树证据，不能直接作为主分支安装包。

集成后的 TypeScript 检查通过；本次 62 个前端文件的 ESLint 通过，摘要渲染 JSX
格式已修正。桌面 `cargo check --bin iyw-claw --locked --offline --jobs 4` 通过，
有既有 Rust warning。复用之前生成的本地 frontend/worker 资源完成检查，未把资源
或 cargo check 的零字节 sidecar 占位文件提交为制品。
`server-runtime,computer-helper` 全 bin 的 locked/offline cargo check 也通过，
有 Rust warning。后续 main 合并须核对合并树与已验证功能分支一致。
JSON、脚本、安装脚本、diff 和凭据模式检查通过；原有 62 个工具 schema 保持一致，
新增 18 个电脑工具，Claw 与 Fusion 六平台归档摘要一致。本轮按仓库规则未运行
Claw 自动化测试，正式安装包、签名及真实桌面操作继续作为后续发布门禁。

## 2026-10-09 驱动统一准备与日常共享入口

用户要求驱动由启动环境准备、移除驱动卡片，新增运行环境设置页，并把日常
窗口共享入口放到对话输入框「＋」菜单，菜单显示由后台电脑操作开关决定。

验收与步骤：

1. 已实施：驱动自动准备 -> 线上既有安装策略 API 已开启，catalog revision 167；
   五平台空库存 resolve 均包含 cua-driver 0.32.0 的 install 动作。客户端启动不再过滤
   未安装驱动；支持电脑操作的宿主会检测缺失组件并进入统一修复，开关与共享仍默认关闭。
2. 已实施：运行环境设置页 -> 静态路径与设置弹窗、导航共用同一页面，Web 设置
   跳转映射已补齐。复用现有 bootstrap 状态/修复/进度接口。
   展示加载、空状态、组件状态、错误、进度与结果，
   防重复点击；修复状态按 Transport 保留，页面切换与宿主切换不混入其他宿主结果。
3. 已实施：对话共享 -> 「＋」菜单仅在后台电脑操作开启且宿主支持时显示，使用既有
   窗口面板的读取/操作授权。面板在菜单外持有，不会随菜单关闭而卸载；没有隐式开启
   开关或默认授权。后台关闭时关闭面板，再次开启不自动打开旧面板。
   设置页共享入口与状态栏保持同一份授权语义。
4. 已验证：最终 TypeScript、定向 ESLint、消息键一致性与新增函数/文件长度检查通过，
   Rust 修改的格式检查通过，静态复核覆盖开关事件、宿主切换、修复写入锁与错误重试。
   message-input 既有 3 条格式错误、2 条 voice hook 警告未增加；保留其他 Windows 7
   与发布工作树修改。Fusion 安装策略回归、相关 Go test/vet 与全量 build 已通过。

本轮没有新增或运行 Claw 自动化测试，没有再次编译或发布桌面应用。
尚未实机验证「＋」菜单点击、窗口授权与运行环境修复，正式安装包仍需完成这些验证。

用户已授权本轮提交、推送至远程主分支。提交范围限定为驱动自动准备、运行环境
设置页、对话共享入口及配套文档；其他 Win7、安装器、发布和 PDF 修改保持原样。
提交前在隔离工作树复核待提交内容，按仓库约定不运行 Claw 自动化测试或桌面构建。

## 2026-10-09 发布预检兼容新组件

0.1.262 五个平台绑定创建成功，但初始化策略开启后，计划中的 cua-driver 被发布
预检旧允许列表拒绝。补齐允许列表并让未知组件错误包含组件 ID；继续保留平台、
必需组件、制品身份、摘要及 HTTPS/TOS 域名检查，不放宽其他组件。
通过同一线上只读 resolve 预检验证五个平台；不重新创建绑定，不发布应用。
