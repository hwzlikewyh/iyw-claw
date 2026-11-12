# 原生插件兼容

## 目标与边界

原助理通过通用导入器接入包含 `.codex-plugin/plugin.json` 或
`.claude-plugin/plugin.json` 的插件，Tripo Studio 是首个验证对象。
导入器读取原生清单与真实 stdio MCP 的工具、资源契约，生成现有 Plugin v2
目录，再由原助理现有插件市场的文件夹上传、安装、权限授权和卸载流程处理。
导入会启动插件服务以读取契约；不会调用生成、登录、付费或写入工具。

兼容范围是随源包携带构建产物、通过 Node.js stdio 运行的插件。支持固定 npm
包对应的本地 `bin` 入口及 `node <entrypoint>`，不在宿主运行时执行 npx 下载。
多 connector、插件技能、模型可见工具、应用专属工具、MCP Apps 界面均沿用
HostGateway。HTTP/SSE、shell 命令、安装脚本、hooks 和子进程参数的其他形式
明确报错，不静默降级。这里的原生插件是编码 Agent 插件格式，不表示任意平台插件。

## 方案选择

直接复用 Codex 的安装目录会依赖特定 Agent，也不能覆盖服务器模式；另建插件
运行时会绕开现有权限与生命周期。采用导入时转换、运行时复用的方案，使现有
v1/v2 清单、安装计划和数据库结构保持兼容。

运行参数、原生来源、应用可见性作为随制品校验的兼容元数据保存，不增加共享
manifest 字段。工具 schema 来自真实 `tools/list`，运行时继续执行现有契约校验。
应用资源逐项绑定，不允许通过资源 URI 访问另一个 connector。

## 执行计划

1. 已完成：原生清单读取、受限入口解析、MCP 握手和 v2 目录生成
   → Node 语法检查、使用 Tripo 发布包读取实际工具/资源契约。
2. 已完成：宿主运行参数、工具可见性和资源读取兼容
   → 沿调用链静态审查，检查环境、超时、权限、资源绑定和关闭流程。
3. 已完成：MCP Apps 桥接和大资源传输
   → 校对正式 MCP Apps 规范和 Tripo Codex host 检测；前端 lint、格式、TypeScript
     检查通过。
4. 进行中：提交前静态审查与桌面检查
   → 服务器模式 Cargo 检查通过。原主分支 MCP connector 管理的 `confirm` 可见性
     修复已纳入提交候选。Next.js 静态构建长时间不产生 `out/`，本次构建随后结束；
     Tauri 资源打包和真实账号/付费操作未验证。

## 验收标准

- 同类原生插件能够通过同一导入器转换，并使用现有市场入口安装。
- 技能和模型可见工具可发现；应用专属工具只允许关联应用调用。
- 工作台、结果卡片、图片导入和最大 32 MB 的 3D 预览具有足够传输容量。
- 环境变量只传入原生清单声明的名称；保持宿主凭证隔离与已有权限授权。
- 密码、Cookie、Token 与 API key 只能引用宿主变量；原生 `.mcp.json` 不复制进上传包。
- 桌面、服务器共享同一业务实现，既有 v1/v2 包不依赖兼容元数据。

## 验证约定

遵循仓库 AGENTS.md，不新增测试文件或运行自动化测试套件。使用静态调用链审查、
语法/lint/类型/编译检查以及真实 MCP 只读握手验证；未进行真实用户登录、
扣费生成及原生界面操作前，不宣称这些流程已经端到端通过。

## 使用入口

开发仓库中运行：

```bash
node scripts/import-native-plugin.mjs --source <native-plugin-directory> --output <output-parent-directory>
```

安装后的应用由内置 `plugin-creator` 技能提供同一导入脚本。输出目录自动使用
插件名，选择新的输出父目录即可。随后在插件市场现有上传入口选择整个输出目录，
再安装并授权。不会自动发布或安装到用户账户。

Node.js 最低版本为 22；导入器不安装依赖，不运行生成或登录工具。必须使用已构建的
源包。环境变量在运行原助理的主机上设置，仅传入原生清单声明的插件变量和基础
运行目录；已声明且以 `_DATA_DIR` 结尾的插件变量指向当前账号会话/企业空间范围的插件私有目录，不再继承主机同名目录跨账号共享。HOME/USERPROFILE/APPDATA/XDG 同样使用该范围的私有子目录，主机原有凭据不自动迁入。宿主及模型提供商的凭证前缀不会透传。服务器端的登录和 Blender 均指
服务器本机，不能直接使用浏览器客户端上的本地数据。

2026-11-06：正式市场详情增加安装依赖、项目权限与连接检查。权限确认展示的是已安装版本；权限或版本变更后旧确认不能提交，撤权覆盖当前项目的继承开关并回收运行时和界面。`current.json` 指针另存受管安装依赖及账号范围摘要，跟随既有版本事务回退；不改签名清单、不保存令牌。旧v2缺少可核对快照时显示修复入口，不能自动猜测依赖或业务登录状态。

未声明业务认证时，连接检查调用初始化和只读目录方法，核对所声明工具/schema/资源。声明`preparation.authentication`后，还会调用对应只读status确认账号；登录与断开由用户显式发起。参数复用keyring并按账号/企业空间/项目注入原runtime，完整声明与使用见`plugin-preparation.md`。第三方系统钥匙串和实际业务认证行为仍需实机核对，完整阶段记录见 `capability-market-implementation.md`。

## Tripo 0.3.13 只读接入记录

- 从 npm 固定版本发布包读取原生清单，从复制后的制品执行真实 MCP 握手。
- 生成 1 个技能、1 个 Node 运行时、1 个 connector、68 个 capability、29 个 app 绑定。
- 包内实际工具声明包含 62 个模型可见工具和 67 个应用可见工具，按声明保留可见性。
- 转换后的运行入口再次握手，68 个工具的输入 schema 与导入契约逐项一致。
- 通过 MCP 读取 `workbench`（2,341,742 字节）和 `result-card`（2,249,737 字节）。
- 未使用真实账号登录，未调用付费生成、导出或 Blender 操作，未发布到外部市场。
- 交付目录有 100 个文件、25,052,103 字节，低于 Skill Market 的 25 MiB 与 512 文件上限；原生清单和 `.mcp.json` 不会随包发布。

协议依据为 MCP Apps 2026-01-26 正式规范及 Tripo 仓库当前 `.mcp.json`、
`src/server.mjs` 和 `ui/workbench-reload.mjs`。界面握手采用标准 UI 扩展能力，
兼容 OpenAI MCP Apps 的 `hostInfo.client_type = "codex"` 标记，让 Tripo 自带的
Codex 宿主识别逻辑可以初始化。工具结果通过通知的 `params` 直接传入；64 MB 桥接上限容纳 32 MB 模型的 base64
内容，资源 HTML 保持独立的 8 MB 上限，`data:`/`blob:` 仅用于本地资源读取。

## ZCode 插件移植核对（2026-11-06）

用户明确希望修改ZCode插件为原助理插件，非原样加载其生态。已只读检查本地zcode-plugins-official缓存的20种插件/22个版本目录。文档四件套有独立stdio MCP和viewer，可优先改启动、工作区、工具/应用注册与导出接口；富编辑器另需WASM/大包/分块支持。浏览器/电脑控制依赖ZCode核心注入的宿主桥接，图片搜索与建站依赖其账号/云服务，需要分别替换。

详细可复用内容、修改范围和当前代码接点见 D:/projects/iyw/reports/zcode-plugin-port-20261106/assessment.md。初次核对为静态检查；后续只读握手见下，不以握手代替移植、发布或端到端可用性。

### 文档四件套真实只读握手

11月6日续做已在隔离HOME/APPDATA、空工作区和最小环境下，使用临时Node入口显式调用各自的main，读取initialize、tools/list、resources/list、resources/templates/list及面板resources/read。未修改源缓存，未调用任何业务工具、登录或生成操作，未上传或安装。

四个插件均返回15个工具（5个模型可见、11个应用可见，其中open_document共用），各1个面板及2个动态资源模板。面板约605KB，摘要和真实契约归档于 D:/projects/iyw/reports/zcode-plugin-port-20261106/read-only-capture/。

动态资源包括各自命名空间的assets/{+name}和file/{token}/{index}；当前宿主只允许精确URI，不能直接读取这些资源。资源还声明zcode/csp.wasmUnsafeEval，前端SDK含2026-07-28协议，必须在独立移植包中明确适配和验证，不能仅替换清单或放开全部URI/CSP。云端发布、用户自选安装、真实文档导入/预览/导出尚待执行。


### 原助理独立文档移植包

现已从只读源缓存生成iyw-documents/iyw-pdf/iyw-spreadsheets/iyw-presentations 0.1.0候选包，作者入口见plugins/office-ports/README.md；未发布或预装。宿主支持受限ui动态模板、按包/资源声明的WASM、安全srcdoc沙箱、可信对话元数据和侧栏结果同步。原生元数据新字段默认关闭，旧制品行为保留。

本地真实文件已打开/读取/搜索，表格命名空间和模块Worker、DOCX初次缩放等问题已在移植制品修复。最终包、证据和未完成范围见reports/capability-market-implementation-20261105/office-ports/README.md。发布前必须沿现有min_client_version绑定包含上述宿主支持的客户端正式版本；当前不能让旧宿主安装后才报不支持。
