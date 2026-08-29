# Infinite Canvas 本地插件接入设计

日期：2026-08-29

状态：已按用户“开始吧”采用推荐方案，等待书面设计校对

设计基线：`iyw-claw@b43ebe63bbb49b3daa4db53fa247de924e81533a`

上游基线：`basketikun/infinite-canvas@ed013e8e5ce8ccab47cf2fc779f8e94555eb4c23`

## 1. 目标

将 `basketikun/infinite-canvas` 适配为 iyw-claw 可按需安装的 v2 插件，作为 Cowart 的
无 tldraw 替代方案。用户在对话中调用插件后，当前回复内打开原生 MCP Apps Widget；
Agent 和用户操作同一份项目级画布，刷新、历史恢复、升级和卸载均不丢画布成果。

最终方案必须满足：

1. 插件不进入 iyw-claw 原生安装包，只在用户使用时下载和授权；
2. Widget、MCP runtime 和持久化全部在本机运行，不依赖 `canvas.best`；
3. 运行代码和生产依赖不包含 tldraw，也不需要商业 license key；
4. Agent 可读取画布、选区和视口，并可批量创建、修改、连接和删除节点；
5. 用户可编辑文本、图片、视频、音频、分组、HTML、Markdown 和 SVG 节点；
6. 图片生成、标注编辑和 Slides 能力在同一插件架构中补齐，不另建平行宿主；
7. 插件禁用或卸载后保留 workspace 中的画布和资产；
8. 后续同类插件复用通用 Plugin v2 和 MCP Apps Host，不增加产品级插件特判。

## 2. 已验证事实

### 2.1 iyw-claw 宿主

当前主线已包含：

- v2 `.iyw-plugin.json` 解析和 runtime、connector、capability、app 绑定；
- workspace/Agent 级首次授权、HostGateway 懒启动和 runtime 回收；
- `ui://` resource 读取、持久 app instance、短期 lease 和历史恢复；
- inline/fullscreen Widget、MCP Apps MessageChannel bridge；
- Widget `tools/call`、`resources/read`、`ui/message`、剪贴板和打开链接授权。

因此本任务优先只增加插件包。只有真实适配证明通用宿主缺少必要能力时，才修改宿主，
且修改必须服务所有 MCP Apps 插件，不能出现 `if plugin == infinite-canvas`。

### 2.2 Infinite Canvas 上游

上游具备无限画布、多项目、节点连线、图片/视频/音频/文本、生成工作流、Canvas Agent、
MCP 工具和节点插件 SDK。画布核心是 React/DOM 自有实现，代码搜索未发现 tldraw 或
Excalidraw 依赖。

本机从固定 commit 构建成功，Vite 产物共 14 个文件、3.65 MiB，主 JS 约 3.70 MB，
低于 iyw-claw 单 Widget HTML 8 MiB 上限。上游 npm lock 与 package.json 当前不同步，且
Ant Design peer 版本冲突，因此正式插件不能直接复用其 npm 安装结果；必须生成、提交并
验证自己的锁文件。

仓库根 LICENSE 和 `@basketikun/canvas-agent` 声明 MIT，但上游
`.codex-plugin/plugin.json` 写为 `AGPL-3.0`。v2 插件包不包含该原生 Codex manifest；
发布前仍需保留根 MIT 文本、上游版权和依赖清单，并把许可证冲突作为必须关闭的审计项。

### 2.3 不能直接原样安装的原因

上游插件打开独立网页，并通过 loopback Canvas Agent 连接 MCP。iyw-claw 的正式 Widget
运行在隔离 sandbox 中；直接沿用会引入端口、HTTP loopback、Origin、token 和额外进程，
同时上游画布主要写浏览器 IndexedDB，无法保证项目级保存或卸载后可恢复。

本设计不开放 loopback 例外，而是将 Widget 与 MCP runtime 改为同一项目文件协议。

## 3. 方案选择

### 3.1 采用：本地单 HTML Widget + workspace MCP runtime

将上游画布 UI 建成小于 8 MiB 的自包含 HTML resource。Widget 不直接访问文件系统、
网络或本地端口，只通过 MCP Apps bridge 调用同一 connector 的工具。runtime 负责 schema
校验、串行化、文件锁、资产读写和 Agent 工具。

优点是安装无感、离线可用、权限清晰、数据属于项目，并完全复用当前 HostGateway。

### 3.2 不采用：本地 Web 服务

该方案可少改上游页面，但需要 HTTP loopback、端口分配、token 传递、CSP 例外、服务
发现和第二套生命周期。它扩大安全面，也让“Widget 已打开”不等于“画布服务可用”。

### 3.3 不采用：线上 `canvas.best`

该方案依赖网络、上游部署、线上版本和浏览器存储，无法保证企业商业交付、离线使用、
项目级成果和确定升级，因此不进入正式路径。

## 4. 插件包结构

插件源码放在 iyw-claw 仓库的 `plugins/infinite-canvas/`，不参与桌面/服务端默认构建：

```text
plugins/infinite-canvas/
├── .iyw-plugin.json
├── LICENSE
├── THIRD_PARTY_NOTICES.md
├── upstream.json
├── contracts/
├── skills/
├── runtime/
│   ├── src/
│   └── dist/infinite-canvas-mcp.mjs
├── widget/
│   ├── src/
│   └── dist/infinite-canvas-widget.html
├── vendor/infinite-canvas-web/
├── package.json
├── pnpm-lock.yaml
└── scripts/
    ├── build.mjs
    ├── package.mjs
    └── verify.mjs
```

`vendor/` 保存实际参与构建的上游源码和许可证，避免发布时临时联网拉取或只提交补丁而
缺源码。`upstream.json` 记录仓库、commit、同步时间和本地 patch 列表。生成产物也进入
插件目录，Fusion artifact 可以在没有开发依赖时直接运行。

v2 artifact 只包含 `.iyw-plugin.json`，不包含上游 `.codex-plugin`、`.mcp.json`、安装脚本
或任何会绕过 HostGateway 的入口。

## 5. 组件和稳定能力

插件 slug 使用 `infinite-canvas`。首版 manifest 声明：

- runtime：`infinite-canvas-node`，bundled Node entrypoint；
- connector：`infinite-canvas-mcp`，stdio、host_gateway、workspace lazy；
- app：`infinite-canvas-canvas`，资源
  `ui://widget/infinite-canvas/canvas.html`，inline/fullscreen；
- skill：`infinite-canvas-open`、`infinite-canvas-edit`、
  `infinite-canvas-image-workflow`。

稳定 capability 至少包括：

- `plugin.infinite-canvas.canvas.render.v1`；
- `plugin.infinite-canvas.canvas.get-state.v1`；
- `plugin.infinite-canvas.canvas.get-selection.v1`；
- `plugin.infinite-canvas.canvas.apply-ops.v1`；
- `plugin.infinite-canvas.canvas.save-snapshot.v1`；
- `plugin.infinite-canvas.canvas.read-asset.v1`；
- `plugin.infinite-canvas.canvas.write-asset.v1`；
- `plugin.infinite-canvas.canvas.export.v1`。

生成类能力在第二阶段增加，但沿用同一 scene、asset 和 operation contract。Skill 只通过
稳定 IYW capability gateway 调用，不要求动态 `mcp__infinite_canvas__*` 顶层工具。

## 6. 数据模型和持久化

每个 workspace 使用：

```text
canvas/infinite-canvas/
├── index.json
├── canvases/<canvas-id>/scene.json
├── canvases/<canvas-id>/selection.json
├── canvases/<canvas-id>/view.json
└── assets/<sha256>.<ext>
```

`scene.json` 包含 schemaVersion、revision、nodes、connections、groups 和更新时间。所有
媒体节点只保存相对 asset 引用，不把大体积 base64 重复写入 scene。

runtime 对同一 canvas 的写入串行化，并使用临时文件、flush、原子 rename 和 revision
比较。Widget 保存携带 `baseRevision`；冲突时返回最新 revision 和稳定错误码，Widget 先
保留未提交操作，再重新读取并重放。多个 Agent/runtime 进程通过有界文件锁避免互相覆盖。

Widget 启动后读取完整 scene；用户变更按 operation 批量防抖保存。Agent 调用同一
`apply-ops` contract。Widget 在可见时按 revision 轻量同步，隐藏或 teardown 后停止轮询。

旧 Cowart 文件不原地修改。迁移器读取 `canvas/pages/*/cowart-canvas.json`，生成新的
Infinite Canvas scene 和资产引用；迁移成功前不删除 Cowart 数据，无法表达的 shape 写入
迁移报告。

## 7. Widget 行为

Widget 使用上游视觉和交互，但移除与正式插件重复或不适用的部分：独立登录、线上部署、
API Key 配置、Canvas Agent 连接页、站点导航和浏览器项目管理。

保留无限画布、节点、连线、分组、小地图、缩放、撤销重做、导入导出、HTML、Markdown、
SVG 和本地媒体编辑。首次 render 默认打开 workspace 的 `main` 画布；工具输入可明确指定
canvasId。inline/fullscreen 切换复用同一 app instance 和 scene。

Widget 内的生成、标注编辑和 Slides 请求使用 `ui/message` 发送当前选区与结构化意图给
当前 Agent。Agent 完成生成后通过稳定 capability 写回节点和资产。Widget 不保存供应商
API Key，也不直接调用模型服务。

## 8. 权限与安全

首版权限 ceiling：

- workspace read/write：`canvas/infinite-canvas`；
- host：`send-message`、`clipboard-write`、`open-link`；
- network：默认空；
- frame：默认空。

所有路径 canonicalize 后必须位于 workspace 授权目录。资产扩展名不能决定 MIME，runtime
按实际内容校验；限制单资产、scene、单次 operation、MCP message 和 Widget HTML 大小。
HTML 节点使用无同源权限的二级 sandbox，默认无脚本；用户明确开启交互预览时仍不获得
宿主 token、Cookie、文件系统或任意网络。

日志只记录 plugin/version、workspace hash、canvasId、revision、operation 数和稳定错误码；
不记录绝对路径、画布内容、图片数据、用户消息、lease、nonce 或凭证。

## 9. 错误与生命周期

- 未安装：返回 `plugin_install_required`，由宿主展示安装确认；
- 未授权：显示当前 workspace 权限确认，不启动 runtime；
- runtime 启动失败：显示可重试错误，不回退线上页面或本地 Web 服务；
- scene 损坏：保留原文件，读取最后一个有效原子版本并显示恢复提示；
- revision 冲突：不覆盖，重新读取并重放本地操作；
- 资产缺失：保留节点占位和相对引用，允许用户重新关联；
- 禁用/卸载：阻止新调用、撤销 lease、回收 runtime，保留 workspace 数据；
- 升级：新 instance 使用新版本，已有 instance 固定旧版本直到 teardown。

## 10. 实施阶段与完成定义

阶段一建立可交付的本地原生画布：插件包、Widget、项目持久化、MCP 读写、安装授权、
inline/fullscreen、恢复、禁用和卸载闭环。

阶段二接通图片生成、图片插入、标注编辑、HTML/Markdown/SVG 节点和导入导出，使常用
Cowart 创作流程不需要旧插件。

阶段三实现 Slides 等价节点、Cowart 非破坏迁移、真实安装客户端回归和 Fusion 发布。

阶段划分只用于降低交付风险，不缩小最终目标。以下证据全部具备前不得宣称完成：

1. v2 verifier、文件清单、SHA-256、MIT/第三方许可证审计通过；
2. artifact 从 Fusion 安装，首次授权、冷启动和 warm reuse 成功；
3. 当前回复真实显示本地 Widget，inline/fullscreen 同 instance；
4. 用户和 Agent 分别修改同一画布，刷新和历史恢复后内容一致；
5. 图片、HTML、Markdown、SVG、标注编辑和 Slides 真实可用；
6. Cowart 样例迁移不修改原文件，迁移报告可检查；
7. 禁用、升级、卸载和退出无残留 runtime，workspace 数据仍存在；
8. 正式 Windows 安装客户端完成端到端验证；其它平台未验证时明确标注；
9. Fusion 普通用户目录可发现、安装和再次启动该插件。

## 11. 回滚

插件发布失败时只下架 Infinite Canvas 版本，不回滚通用 Host，也不删除用户画布。宿主若
必须增加通用能力，应独立提交并可在不影响其它插件的情况下回滚。迁移失败始终保留
Cowart 原文件；用户可继续使用已安装的旧插件，直到新插件完成真实验收。
