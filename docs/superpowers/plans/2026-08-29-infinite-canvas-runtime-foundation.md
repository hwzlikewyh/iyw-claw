# Infinite Canvas Runtime Foundation Implementation Plan

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交付可被 iyw-claw Plugin v2 安装和懒启动的 Infinite Canvas runtime、稳定能力、项目持久化与可复现 artifact。

**Architecture:** 一个 bundled Node MCP 进程以 `IYW_WORKSPACE_DIR` 为唯一 workspace 根目录，通过原子 JSON、内容寻址资产和 revision 操作协议持久化画布。工具 schema 和 Fusion contract 文件由同一 TypeScript 常量生成，避免运行时与 manifest 漂移。

**Tech Stack:** Node.js、TypeScript、MCP SDK 1.29.0、Zod 3.25.76、esbuild 0.25.12、pnpm 11、Plugin v2。

## Global Constraints
- 执行主计划 `2026-08-29-infinite-canvas-plugin.md` 的全部 Global Constraints。
- artifact 中 entrypoint、schema、Skill、Widget 和 LICENSE 必须自包含；运行时不安装依赖。
- 画布 ID 只允许 `[A-Za-z0-9_-]{1,64}`，默认 `main`。
- scene 写入始终使用 revision、临时文件、flush 和原子 rename；冲突不覆盖。

---
### Task 1: 建立插件源码、来源和锁文件
**Files:**
- Create: `plugins/infinite-canvas/package.json`
- Create: `plugins/infinite-canvas/pnpm-workspace.yaml`
- Create: `plugins/infinite-canvas/pnpm-lock.yaml`
- Create: `plugins/infinite-canvas/tsconfig.json`
- Create: `plugins/infinite-canvas/upstream.json`
- Create: `plugins/infinite-canvas/LICENSE`
- Create: `plugins/infinite-canvas/THIRD_PARTY_NOTICES.md`
- Create: `plugins/infinite-canvas/vendor/infinite-canvas-web/**`

**Interfaces:** Consumes 上游 commit `ed013e8e5ce8ccab47cf2fc779f8e94555eb4c23`；produces 可离线构建的完整源码、固定依赖和上游 provenance。

- [x] **Step 1: 导入固定上游源码**

使用唯一临时目录 clone 固定 commit；只机械复制 `web/src`、`web/public`、`plugins/canvas`、
根 LICENSE 和相关 package metadata，不复制 `.git`、`node_modules`、`dist` 或上游 Codex manifest。

```powershell
$source = Join-Path $env:TEMP ("infinite-canvas-vendor-" + [guid]::NewGuid().ToString("N"))
git clone --filter=blob:none https://github.com/basketikun/infinite-canvas.git $source
git -C $source checkout --detach ed013e8e5ce8ccab47cf2fc779f8e94555eb4c23
New-Item -ItemType Directory -Force -Path "plugins\infinite-canvas\vendor\infinite-canvas-web" | Out-Null
Copy-Item -Recurse -LiteralPath "$source\web\src" -Destination "plugins\infinite-canvas\vendor\infinite-canvas-web\src"
Copy-Item -Recurse -LiteralPath "$source\web\public" -Destination "plugins\infinite-canvas\vendor\infinite-canvas-web\public"
Copy-Item -Recurse -LiteralPath "$source\plugins\canvas" -Destination "plugins\infinite-canvas\vendor\infinite-canvas-web\canvas-plugins"
```

- [x] **Step 2: 写固定构建元数据**

`package.json` 使用 exact versions，并提供唯一入口：

```json
{
  "name": "@iyw/infinite-canvas-plugin",
  "version": "0.1.8",
  "private": true,
  "type": "module",
  "packageManager": "pnpm@11.18.0",
  "scripts": {
    "build": "node scripts/build.mjs",
    "typecheck": "tsc --noEmit",
    "verify": "node scripts/verify.mjs",
    "package": "node scripts/package.mjs"
  },
  "dependencies": {
    "@modelcontextprotocol/ext-apps": "1.7.5",
    "@modelcontextprotocol/sdk": "1.29.0",
    "zod": "3.25.76"
  },
  "devDependencies": {
    "@types/node": "22.19.7",
    "esbuild": "0.25.12",
    "typescript": "5.8.3",
    "vite": "7.3.6",
    "@vitejs/plugin-react": "5.2.0"
  }
}
```

- [x] **Step 3: 生成并冻结 pnpm lock**

Run: `pnpm install --lockfile-only --dir plugins/infinite-canvas`

Expected: exit 0；lockfile 只引用 exact direct versions，不执行上游 lifecycle script。

- [x] **Step 4: 写 provenance 和许可证**

`upstream.json` 精确记录 repository、commit、license、vendoredPaths、localPatches；
`THIRD_PARTY_NOTICES.md` 说明上游根 MIT 与错误的原生插件 AGPL 元数据未被打包，并列出
生产依赖许可证审计命令。

- [x] **Step 5: 验证导入边界**

Run: `rg -n -i "tldraw|canvas\.best|agentToken" plugins/infinite-canvas/vendor`

Expected: 无 tldraw；`canvas.best` 和 Canvas Agent 连接代码只作为待裁剪源码出现并在 Widget
入口不可达，后续 Task 会从 bundle 中排除。

### Task 2: 定义 manifest、稳定 schema 和 Skill
**Files:**
- Create: `plugins/infinite-canvas/.iyw-plugin.json`
- Create: `plugins/infinite-canvas/runtime/src/contracts.ts`
- Create: `plugins/infinite-canvas/scripts/generate-contracts.mjs`
- Create: `plugins/infinite-canvas/contracts/*.schema.json`
- Create: `plugins/infinite-canvas/skills/infinite-canvas-open/SKILL.md`
- Create: `plugins/infinite-canvas/skills/infinite-canvas-edit/SKILL.md`
- Create: `plugins/infinite-canvas/skills/infinite-canvas-image-workflow/SKILL.md`

**Interfaces:** Produces manifest capability ID ↔ toolName ↔ schemaPath 一一映射。

- [x] **Step 1: 写 v2 manifest**

Manifest 使用 `schemaVersion: 2`、`targets: ["iyw-claw"]`，声明 bundled Node runtime、
workspace lazy connector、三个 Skills、九个主能力和一个 app。app 绑定
`render-canvas`，resource 为 `ui://widget/infinite-canvas/canvas.html`；workspace read/write
仅 `canvas/infinite-canvas`，host 为 `send-message`、`clipboard-write`、`open-link`，network
所有域数组为空。

- [x] **Step 2: 写单一 schema 源**

`contracts.ts` 导出以下不可变映射；每个 schema 根必须是 `type: "object"`：

```ts
export const contracts = {
  render_infinite_canvas_widget: object({ canvasId: canvasId(false), displayMode: enumOf(["inline", "fullscreen"], false) }),
  get_infinite_canvas_state: object({ canvasId: canvasId(false), sinceRevision: integer(0, false) }),
  get_infinite_canvas_selection: object({ canvasId: canvasId(false) }),
  save_infinite_canvas_selection: object({ canvasId: canvasId(false), revision: integer(0, true), selectedNodeIds: arrayOf(idSchema, 0, 200) }),
  apply_infinite_canvas_ops: object({ canvasId: canvasId(false), baseRevision: integer(0, true), operations: arrayOf(operationSchema, 1, 200) }),
  save_infinite_canvas_snapshot: object({ canvasId: canvasId(false), baseRevision: integer(0, true), scene: sceneSchema }),
  read_infinite_canvas_asset: object({ sha256: sha256(true), offset: integer(0, false), length: integer(1, false) }),
  write_infinite_canvas_asset: object({ uploadId: id(false), sourcePath: text(500, false), name: text(180, false), mimeType: text(120, false), expectedBytes: integer(1, false), expectedSha256: sha256(false), chunkIndex: integer(0, true), dataBase64: text(174_768, false), finalize: boolean(false) }),
  export_infinite_canvas: object({ canvasId: canvasId(false), format: enumOf(["json", "png", "svg"], true), sourceAssetSha256: sha256(false), fileName: text(180, false) }),
} as const
```

- [x] **Step 3: 生成 contract 文件**

`generate-contracts.mjs` 从映射生成 kebab-case `contracts/<key>.schema.json`，结尾换行；运行两次
后 `git diff` 必须为空。

Run: `node plugins/infinite-canvas/scripts/generate-contracts.mjs`

Expected: 10 个非空 schema 文件（含已接入的 Cowart migration capability）。

- [x] **Step 4: 写三份 Skill**

Skills 只使用 stable IYW capability gateway。open Skill 在插件未安装/未授权时走宿主安装或
授权确认；edit 先 get-state/get-selection 再 apply-ops；image-workflow 生成结果后使用
write-asset/apply-ops，不搜索原生 MCP namespace、不启动 Web 服务、不要求新会话。

### Task 3: 实现项目级 scene、operation 和资产存储

**Files:**
- Create: `plugins/infinite-canvas/runtime/src/types.ts`
- Create: `plugins/infinite-canvas/runtime/src/errors.ts`
- Create: `plugins/infinite-canvas/runtime/src/paths.ts`
- Create: `plugins/infinite-canvas/runtime/src/lock.ts`
- Create: `plugins/infinite-canvas/runtime/src/scene-store.ts`
- Create: `plugins/infinite-canvas/runtime/src/operations.ts`
- Create: `plugins/infinite-canvas/runtime/src/asset-store.ts`

**Interfaces:** Produces `SceneStore.read/save/apply`、`AssetStore.begin/writeChunk/finalize/readChunk`。

- [x] **Step 1: 定义 scene 和 operation 类型**

沿用主计划的 `CanvasScene`/`CanvasOperation`，增加 `CanvasSelection`、`AssetRef` 和
`RevisionConflict { code: "revision_conflict"; latestRevision: number }`。节点 ID、connection
ID 和 canvasId 在解析边界验证，patch 禁止 `__proto__`、`prototype`、`constructor`。

- [x] **Step 2: 约束 workspace 路径**

```ts
export function storageRoot(): string {
  const workspace = requiredEnv("IYW_WORKSPACE_DIR")
  return resolve(workspace, "canvas", "infinite-canvas")
}
export function canvasRoot(canvasId = "main"): string {
  if (!/^[A-Za-z0-9_-]{1,64}$/.test(canvasId)) throw invalid("canvas_id_invalid")
  return assertWithin(storageRoot(), resolve(storageRoot(), "canvases", canvasId))
}
```

- [x] **Step 3: 实现原子 scene 保存**

`SceneStore.save(scene, baseRevision)` 在有界锁内重新读取磁盘 revision；不匹配抛
`revision_conflict`。写 `<scene>.tmp-<pid>-<nonce>`、`FileHandle.sync()`、rename，成功后返回
`revision + 1`。同一锁内更新 `index.json` 的 canvasId/title/updatedAt；任何失败保留现有
scene，并清理由本次调用创建的临时文件。

- [x] **Step 4: 实现确定性 operations**

`applyOperations(scene, operations)` 按数组顺序执行；缺失目标、重复 ID、悬空连接、非法
patch 全部在写盘前拒绝。删除节点同时删除关联 connection；viewport 只接受有限数值和
`0.05 <= k <= 5`。

- [x] **Step 5: 实现分块资产上传**

每块解码后最多 128 KiB，不设置单文件总大小上限。`begin` 在插件 data 临时目录
创建随机 upload 文件；`writeChunk` 强制连续 index；`finalize` 校验精确 size 和 SHA-256，
再原子移动到 `assets/<sha256>.<normalized-ext>`。失败或超时上传不能进入 scene。
`sourcePath` 分支只接受 workspace 内相对文件，读取前拒绝 symlink 和无法读取的文件，
用于 Agent 将已生成成果导入画布；它与 chunk 字段互斥。

- [x] **Step 6: 静态审查存储边界**

检查 path traversal、symlink、跨进程锁、异常释放、Windows rename、重复 finalize、损坏
scene 和 revision conflict；只记录 canvasId/revision/字节数/错误码。

### Task 4: 实现 MCP server、Widget resource 和 app tool 路由

**Files:**
- Create: `plugins/infinite-canvas/runtime/src/server.ts`
- Create: `plugins/infinite-canvas/runtime/src/tool-handlers.ts`
- Create: `plugins/infinite-canvas/runtime/src/resource.ts`
- Create: `plugins/infinite-canvas/widget/src/mcp-app.ts`
- Create: `plugins/infinite-canvas/widget/src/runtime-diagnostic.ts`
- Create: `plugins/infinite-canvas/widget/dist/infinite-canvas-widget.html`

**Interfaces:** Consumes `contracts`、`SceneStore`、`AssetStore`；produces stdio MCP tools/list、tools/call、resources/list、resources/read。

- [x] **Step 1: 注册 raw JSON schema 工具**

使用 MCP SDK low-level `Server`，`ListToolsRequestSchema` 直接返回 `contracts` 中的 schema，
保证宿主 contract equality；`CallToolRequestSchema` 只分派白名单 handler，错误返回稳定 code
和 `isError: true`，stdout 只写 MCP 帧。

- [x] **Step 2: 实现 render 和状态工具**

`render_infinite_canvas_widget` 确保 canvas 存在并返回 revision/canvasId；`get-state` 支持
`sinceRevision` 未变化时只返回 revision；selection/view 分别落盘；apply/save 返回最新
scene 摘要且不把完整媒体写回工具结果。export(json) 原子写 scene JSON；export(png/svg)
要求 Widget 先提交 `sourceAssetSha256`，runtime 再复制到 exports 并返回 workspace 相对路径。

- [x] **Step 3: 返回自包含 Widget resource**

`resources/list` 只声明一个 `text/html;profile=mcp-app` resource；`resources/read` 从
`IYW_PLUGIN_ROOT/widget/dist/infinite-canvas-widget.html` 读取，验证不超过 8 MiB，再返回
resource metadata：network/frame CSP 为空，clipboardWrite 为 true。

`runtime-diagnostic.ts` 使用官方 `App` 在 connect 前注册 tool input/result handler，读取 launch
canvasId 后调用 get-state，并显示 canvasId/revision/runtime ready。它是 foundation 的真实
诊断界面，Native Widget 计划在不改变 bridge 接口的前提下替换其 root UI。

- [x] **Step 4: 接入 stdio transport**

```ts
const server = createInfiniteCanvasServer()
await server.connect(new StdioServerTransport())
```

捕获 `SIGINT`/`SIGTERM`，停止接受新上传并关闭临时文件；禁止 `process.chdir()` 和继承
workspace 外路径。

### Task 5: 建立 build、artifact 和 verifier

**Files:**
- Create: `plugins/infinite-canvas/scripts/build.mjs`
- Create: `plugins/infinite-canvas/scripts/package.mjs`
- Create: `plugins/infinite-canvas/scripts/verify.mjs`
- Create: `plugins/infinite-canvas/dist/infinite-canvas-0.1.8.zip`

**Interfaces:**
- Produces: deterministic ZIP、SHA-256、文件清单和 verifier receipt。

- [x] **Step 1: bundle runtime**

esbuild 输出单个 ESM `runtime/dist/infinite-canvas-mcp.mjs`，platform=node、target=node20，
banner 不读取网络。构建后扫描 `node_modules`、绝对路径和动态安装命令，artifact 不包含它们。

- [x] **Step 2: 实现 deterministic package**

按 UTF-8 路径排序、固定时间戳、拒绝 symlink，包含 manifest、runtime dist、Widget dist、
contracts、Skills、LICENSE、notices 和 upstream.json；源码保留在 Git，但 runtime artifact
不必包含 vendor/build source。

- [x] **Step 3: 实现 verifier**

Verifier 检查文件数 ≤ 512、展开大小 ≤ 50 MiB、Widget ≤ 8 MiB、所有 schema 与 runtime
`tools/list` 完全一致、resource URI 存在、entrypoint 可启动、无 tldraw 字符串、无原生
Codex manifest、无绝对路径和未知许可证。

- [x] **Step 4: 运行 foundation 门禁**

Run:

```powershell
pnpm --dir plugins/infinite-canvas typecheck
pnpm --dir plugins/infinite-canvas build
pnpm --dir plugins/infinite-canvas verify
pnpm --dir plugins/infinite-canvas package
git diff --check
```

Expected: 全部 exit 0；打印 artifact size、SHA-256、10 tools、1 resource、0 tldraw matches。

- [ ] **Step 5: 提交 foundation**

```powershell
git add -- plugins/infinite-canvas docs/superpowers/plans
git commit -m "feat(plugin): 建立 Infinite Canvas 本地运行时"
```
