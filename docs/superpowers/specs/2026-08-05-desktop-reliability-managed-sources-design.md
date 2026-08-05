# 桌面可靠性、统一下载与交互补全设计

## 1. 状态与关系

- 日期：2026-08-05
- 状态：方案 A 已获用户批准，待书面规格复核
- 涉及仓库：`iyw-claw`、`iyw-fusion-api`、`skill`
- 基础设计：工作区
  `../docs/superpowers/specs/2026-08-01-managed-desktop-distribution-design.md`
- 基础契约：Fusion
  `docs/contracts/2026-08-01-managed-desktop-distribution.md`
- 可执行映射与验收附录：
  `docs/superpowers/specs/2026-08-05-desktop-reliability-managed-sources-acceptance.md`

本文是既有托管分发设计的增量规格，不重建版本中心、更新状态机、Skill
发布源或 Agent storage。需要扩展共享字段时，以新增契约修订完成，不能改变
已冻结 r1 字段的既有语义。

## 2. 目标与完成边界

本轮必须同时完成以下用户可见结果：

1. 消除 Tooltip provider、Tauri callback、拖放路径、OpenCode 下载和
   `tauri.localhost` 客户端异常的已知触发链路。
2. 所有指定 Agent、CLI、运行时和辅助工具均先从 Fusion 获取下载决策。
3. 下载严格按“后端托管制品、国内镜像、原始上游”顺序回退。
4. 桌面普通区域不再出现浏览器原生右键菜单，同时保留应用菜单和编辑操作。
5. 主界面搜索按钮旁提供更新入口、版本红点、更新 Dialog、跳过版本和状态恢复。
6. Skill 市场可真实浏览、下载、安装，并发布到 Agent SDK 能发现的目录。
7. 图片持久化和返回统一使用 URL；单张不超过 20 MiB 可上传，普通文件只保留
   本地路径。
8. 新对话默认选择自动模式；Slash 命令同时出现在输入联想和 `+` 菜单。
9. 星河和远山优先完成中文与能力适配，再覆盖全部受支持 Agent。
10. Windows 打包主程序每次启动申请管理员权限。
11. Windows 下星河的预置提示词包含可实际复制执行的 PowerShell 7 命令规范。

只有逐项验收矩阵全部具备证据后，任务才能声明完成。

## 3. 统一来源与下载

### 3.1 后端职责

扩展现有 Agent Version Center，而非创建新系统。Fusion 为每个组件维护：

- 稳定组件 ID、类型、版本、平台、架构和兼容范围；
- 不可变制品的实际大小、SHA-256、签名和许可证状态；
- TOS/CDN 主制品及按优先级排列的国内镜像、原始上游来源；
- 目录修订、计划过期时间、强制/推荐策略和回滚版本；
- 镜像抓取、摘要验证、隔离和健康状态。

纳入目录的组件必须覆盖：

- Agent：云舟、远山、星河、青岚、赫尔墨斯、开放之爪、流光、逐风、月白、知微；
- 工具：OfficeCLI、Agent Reach、OpenCLI、Open Computer Use MCP；
- 运行时：npm、Node.js、uv/uvx、Git；
- 上述 Agent 使用的 npm、Python/uvx、二进制和辅助 CLI 制品。

原始名称、中文显示名、稳定 component key、包类型和离线激活入口以验收附录
第 1 节为准。目录、日志、inventory 和 resolve 请求使用稳定 key，不使用显示名作为
连接键。

### 3.2 来源契约

`resolve/init-plan` 为每项返回严格分组的 `managed[]`、`cn_mirror[]` 和
`upstream[]`，客户端必须耗尽前一组后才进入下一组。每个来源至少包含：

- `kind = managed | cn_mirror | upstream`；
- 票据换取端点或受限 recipe，不直接暴露对象存储 key；
- 预期大小、摘要、签名、过期时间和 Range 能力；
- 该来源的稳定 ID，供脱敏遥测使用。

客户端正常路径不得自行拼接 GitHub、npm、PyPI 或安装脚本 URL。Fusion
不可达时，客户端先读取最后一次验签成功且仍兼容的目录缓存；无可用缓存时才使用
编译期国内灾备白名单，最后使用原始上游白名单。灾备不能引入目录中未知的组件、
版本或命令 recipe。

来源失败分类、换票刷新次数和停止回退的策略错误由验收附录第 2 节冻结。认证、受众、
版本阻止、客户端不兼容和目录过期不得通过切换来源绕过。

### 3.3 下载状态机

每个来源依次执行：换票据、下载到 staging、校验长度与 SHA-256/签名、校验归档
结构、原子激活。网络失败、票据过期、摘要不符或来源隔离按附录定义处理；摘要不符
必须上报并隔离当前字节，不能把损坏文件当作普通网络失败继续使用。

安装过程只能读取已验证的离线闭包。npm、uvx、Git 和第三方 installer 必须处于
cache-only/offline 模式；缺少依赖就失败，不允许绕过候选下载器联网补包或执行远端
脚本。Fusion 只下发结构化 recipe ID，客户端编译期 allowlist 决定可执行 argv。

所有候选失败后保留当前 active 与 last-known-good。日志只记录 component、version、
source ID、阶段、耗时和脱敏错误，不记录完整短时 URL、token 或文件内容。

## 4. Windows 提权与辅助进程

### 4.1 主程序清单

Windows `iyw-claw.exe` 嵌入 `requestedExecutionLevel=\"requireAdministrator\"`。
清单同时保留 Common Controls v6 依赖；用户拒绝 UAC 后程序不以普通权限继续。
该设置不应用于 macOS、Linux、独立服务端或 MCP sidecar。

验收必须读取最终打包 EXE 的嵌入清单，并实际验证启动令牌处于提升状态，不能只检查
源码 XML 或安装器权限。

### 4.2 中完整性启动器与辅助进程

同一个带管理员清单的 EXE 不能继续兼任 Git credential helper，否则每次 Git
鉴权都会弹 UAC。仅给管理员父进程创建的子程序配置 `asInvoker` 仍会继承提升令牌，
因此公开入口是签名的 `asInvoker` launcher：它先以自身中完整性令牌启动 broker，再用
`ShellExecuteExW(runas)` 启动 `requireAdministrator` 主程序。每次入口调用都执行
`runas`；已有实例也在 UAC 后通过认证单实例通道聚焦，不绕过本次提权确认。直接启动
主 EXE 因缺少可信 broker 而拒绝进入工作区，并提示从正式快捷方式重启。

中完整性 broker 承担普通 Explorer 拖放接收面与 Git credential 服务。credential
helper 拆成独立 `asInvoker` 无凭据 shim；它可能继承调用方完整性，只能持一次性
capability 调 broker RPC，不能访问数据库、凭据库或启动 GUI。主 EXE 删除
`--credential-helper` 模式。用户/session 绑定、管道 DACL、反重放、生命周期和攻击面
以验收附录第 7 节为准。

安装根、data、runtime、agents、skills、inventory、config 和 logs 均从可执行文件
所在安装根解析，更新或提权不能改变其位置。更新只替换 `app` 区。

## 5. 运行可靠性

### 5.1 Tooltip 与客户端错误边界

基础 Tooltip 组件自身提供安全 Provider 边界，同时保留页面级 Provider 对延迟参数
的覆盖。这样主窗口、设置窗口、辅助窗口和 Portal 均不依赖某个特定 layout 才能渲染。

为主要静态导出入口增加错误边界，记录版本、窗口标签、路由和关联 ID，不记录用户
内容。错误界面提供原地重试和诊断入口。错误边界不能掩盖异常；必须修复其捕获到的
确定性根因。

`tauri.localhost` 异常必须从真实发布包分别复现并核对 `/`、`/login`、`/workspace`、
全部 `/settings/*`、`/commit`、`/push`、`/merge`、`/stash`、`/pet` 和 `/pet-panel`。
根级静态错误边界覆盖所有导出 HTML，工作台另有局部边界；fallback 不依赖 i18n、
Tooltip 或业务 Provider。对每条路由记录首个异常堆栈，定位到具体静态资源、Provider、
仅浏览器 API、hydration 或 IPC 生命周期根因后修复。验收不是“出现错误页”，而是
冷启动、刷新和窗口重开均能进入可交互界面，控制台无未捕获异常，错误边界未触发。

附件中的 KaTeX `LaTeX-incompatible input ... mathVsTextAccents` 纳入同一波。复现集覆盖
流式/历史消息、用户/助手消息、重音命令和代码块；修复应在 Markdown/数学分界处保留
原始文本或按 KaTeX 支持的语法规范化，不通过吞掉 warning 或全局关闭严格检查验收。

### 5.2 Tauri 异步生命周期

前端 transport 统一登记进行中的 invoke/listener，组件卸载后忽略迟到结果并幂等释放
监听。长任务由 Rust 后端持有状态，前端只订阅快照，不让 WebView reload 销毁唯一
回调。主动 reload/relaunch 前先保存恢复快照并停止新请求；不会为了消除日志而吞掉
真实 IPC 错误。

### 5.3 文件拖放

Tauri 原生事件、浏览器拖放和 Windows 辅助进程都归一化为同一 payload。路径数组、
坐标、窗口标签和事件阶段先校验，再判断落点。坐标统一到当前 WebView CSS 像素，
处理缩放和高 DPI。错误日志输出可读的错误码和阶段，不再只打印不透明对象。

本地图片进入上传流程；普通文件生成本地路径引用。远程工作区不能访问本地普通文件
时明确拒绝并提示，不静默上传普通文件。

## 6. 更新入口与恢复

复用现有 `UpdateProvider`、共享 Rust 状态机、跳过版本和调度器。在桌面和移动标题栏
的搜索按钮旁加入更新图标：检测到未被跳过的新版本时显示红点；下载、验证和安装中
显示稳定进度状态，动态内容不得改变标题栏尺寸。

点击图标打开独立 Dialog，覆盖检查、版本说明、下载、验证、安装、错误重试、稍后、
跳过当前可选版本和重启。强制更新不显示跳过操作。

Windows updater 会在安装阶段退出进程，因此恢复快照必须在开始安装前保存，而不是
只在 `restart_app` 前保存。版本化快照包括：

- 当前路由、活动工作区与工作台视图；
- 打开的对话、文件标签和活动标签；
- 可恢复的会话 ID、面板开关和布局；
- 未保存编辑缓冲及其磁盘基线；
- 快照 schema、原应用版本、目标版本和时间戳。

快照原子写入应用 data 区，不进入日志；新进程仅消费来源版本和目标版本匹配、未过期
且结构合法的快照。恢复完成后删除。无法恢复的终端进程不伪装为仍在运行，而是恢复
标签并显示需重新启动。恢复失败进入默认工作区并保留诊断，不造成启动循环。

## 7. Skill 市场

客户端列表、详情、版本、文件树、普通 ZIP 下载和安装计划使用验收附录第 3 节冻结的
真实 v2 transport；fixture 仅保留给显式性能测试。新客户端使用 v2 install-plan 与
短时 download ticket，现有 `/skills/artifact-ticket` 作为迁移适配器，旧下载接口只服务
旧客户端并按流量门禁退出。

每个 ZIP 使用实际 `artifactSize` 和 `artifactSha256` 校验，安全解压到 staging，所有
依赖成功后再整体激活。中央受管源与当前 Agent storage 根保持一致，然后按
`SkillStorageSpec` 发布到目标 Agent 的规范目录。symlink 不可用时使用可追踪 copy。

安装结果逐个验证 Agent 发现目录及 ownership marker。任一要求目标发布失败时返回
`partial/failed` 和修复动作，不能仅记录 warning 后显示安装成功。卸载只删除受管链接
和受管副本，不删除用户拥有的同名目录。

目录映射和验证分三级执行：文件 ownership、桌面读取 API、隔离 Agent 会话实际发现。
`SkillStorageSpec` 的首个全局/项目目录及环境变量以验收附录为准；仅检查中央目录或
`acp_list_agent_skills` 不足以证明外部 Agent 已加载 Skill。

## 8. 图片与附件契约

图片服务通过验收附录第 4 节冻结的 upload plan/complete/access ticket/ingest URL
接口，把上游 `b64_json` 或短期第三方 URL 规范化为受控对象存储资产，并对客户端统一
返回稳定逻辑 URL、字符串 asset ID、MIME、字节数、摘要和宽高。客户端持久化逻辑 URL，
不把 Base64 正文或会过期的第三方/对象存储短链写入会话、日志或本地数据库。

图片上传规则：

- 单张原始图片不超过 20 MiB（`20971520` 字节）；客户端预检和服务端 complete/ingest
  均强制边界，PNG、JPEG、WebP、GIF 沿用安全 MIME 检测；
- 桌面本地、Web 和远程桌面均先取得上传票据，再直传对象存储或受控上传端点；
- 上传完成后校验大小和摘要，返回统一资产 URL；
- URL 渲染、编辑和下载经过现有安全远程图片代理，限制协议、重定向、地址范围、
  响应大小和 MIME，防止 SSRF；
- 普通文件不上传，只传本地规范化路径；远端不可达时阻止发送并说明原因。

Agent 适配层以 URL 作为持久模型。目标协议只接受字节时，可在发送边界通过受控代理
临时读取并转换，转换结果不回写持久模型。URL-only 生成图片必须可显示、编辑和下载。
历史 Base64/data URL 在读取时幂等迁移；无效或超限历史内容不得重新发送，详细兼容
行为以附录第 4 节为准。

## 9. 对话模式、命令与中文

新增稳定 UI 模式 `iyw_auto`，所有 Agent 的新对话都显示并默认选择“自动模式”。统一
适配层把它映射到原生自动模式，或映射到该 Agent 已验证的执行模式与应用权限策略；
不能改显示名或回退为数组第一项。没有安全映射时阻止启动并显示兼容性错误。用户已
显式选择的旧会话和自动化设置保持不变。先完成星河、远山映射，再覆盖其余 Agent。

Slash 自动完成与 `+` 菜单读取同一命令目录。`+` 菜单中的“斜杠命令”子菜单支持
搜索、键盘选择和插入，命令名称保持协议原文，说明文字本地化。不可用命令显示禁用
原因，不维护第二份硬编码列表。

所有用户可见 Agent 名称继续通过 `AGENT_LABELS/getAgentDisplayName()` 输出中文别名。
新增状态、错误、按钮、命令说明和空状态至少提供 `zh-CN` 与英文资源；星河和远山位于
能力适配和回归矩阵前两项，但不通过复制逻辑获得特殊实现。

Windows 下星河的新 instruction generation 在 `base_instructions` 追加一次 PowerShell 7
预置规则，非 Windows 或其他 Agent 不注入。每个新会话固定其 generation；旧会话
缺失该字段视为 legacy，恢复时保持原代际。若 codex-acp 恢复仍读取全局 model catalog，
则按 generation 隔离 catalog/profile 或阻止不同代际并发，不能让全局重写改变旧会话。
规则原文、转义边界、示例和 round-trip 验收由附录第 6 节冻结，不能把它重复拼到每次
用户 prompt。

## 10. 原生右键菜单

仅在 Tauri 桌面 shell 注册 capture 阶段 `contextmenu` 策略：普通页面调用
`preventDefault()`，但不停止传播，因此 Radix 应用 ContextMenu 仍可打开。

允许菜单的精确区域只有：应用显式标记的输入框、文本域和 `contenteditable` 编辑器，
以及文件树、会话、标签、终端、自动化、图片和预览等已有应用 ContextMenu 的区域。
这些区域均由应用菜单接管：可编辑区提供复制、剪切、粘贴、全选，能力存在时提供
撤销/重做；只读选区提供复制、全选。宠物窗口的独立应用菜单显式豁免。其他桌面区域
全部禁用浏览器原生菜单，不存在“无应用菜单则放行”的兜底。Web 服务模式不改变浏览器
默认行为。右键、Shift+F10/菜单键及键盘复制、粘贴、选择均需回归。

## 11. 实施波次

1. 稳定性与桌面壳：Tooltip、错误边界、IPC 生命周期、拖放、右键、UAC、辅助进程、
   更新入口和恢复快照。
2. 统一分发：Fusion 来源契约、目录/票据、镜像任务和客户端候选下载器，覆盖全部列名
   Agent、工具及运行时。
3. Skill 市场：真实 v2 transport、artifact ticket、安全安装、Agent 目录验证。
4. 图片与对话：20 MiB URL 资产链路、URL-only 渲染、自动模式、共享命令目录。
5. 星河 Windows PowerShell 预置提示词、全 Agent 中文适配、灾备演练、跨仓契约核对
   和完整验收。

每波独立 feature flag、可回滚且不得改变后一波契约。共享 DTO、路由和配置由主任务
串行修改，子任务不并发编辑同一 contract/schema/types 文件。

## 12. 验证与验收证据

桌面仓本机遵守禁止编译、打包、启动和 Cargo 检查的规则，只执行静态调用链审查、
TypeScript/ESLint 定向检查、格式检查、JSON 解析和 `git diff --check`。Windows 动态项
由远端 CI 或发布机验证：

- 最终 EXE 清单、UAC 接受/拒绝、提升令牌、credential helper 不弹 UAC；
- launcher/broker/helper 的中完整性、同用户同 session 绑定和直接启动主 EXE 的降级；
- 普通 Explorer 向管理员窗口拖放，含高 DPI、多窗口、文件夹和拒绝路径；
- 安装、更新、重启、快照恢复和持久目录摘要不变；
- 签名、NSIS 更新、旧版本升级和干净安装。

Fusion 执行格式、构建、相关包测试和契约测试；Skill 发布源执行 manifest、SemVer、依赖
图和制品校验。故障注入必须证明每个组件按 managed、cn_mirror、upstream 顺序回退，
所有失败时保留 active/LKG。

最终需求矩阵至少保存以下证据：异常复现消失、每个命名组件的 resolve 和下载日志、
更新红点/Dialog/跳过/恢复、原生与应用右键菜单、Skill 实际发现、20 MiB 边界与
URL-only 图片、普通文件零上传、自动模式、两处命令入口、全部 Agent 中文映射以及
Windows 每次启动 UAC、星河 Windows 提示词单次注入与模板实跑。Agent/组件逐项证据
以验收附录矩阵为准；任何一项缺少直接证据均视为未完成。
