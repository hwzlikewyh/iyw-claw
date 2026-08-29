# Infinite Canvas Native Widget Implementation Plan

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将上游 Infinite Canvas 画布变成小于 8 MiB、无网络和 IndexedDB 依赖、可在 iyw-claw inline/fullscreen 中读写同一项目 scene 的原生 Widget。

**Architecture:** Widget 使用 MCP Apps 官方 `App` 客户端调用 runtime capabilities；Vite alias 将上游项目存储和媒体存储替换为项目级适配器。上游 UI 保持 vendored，新增代码以小型 bridge、store、asset 和 shell 单元组合。

**Tech Stack:** React 19.2.5、Vite 7.3.6、`@modelcontextprotocol/ext-apps` 1.7.5、上游 Infinite Canvas UI、MCP Apps bridge。

## Global Constraints

- 先完成 `2026-08-29-infinite-canvas-runtime-foundation.md`。
- 执行主计划的全部 Global Constraints。
- Widget bundle 不得包含站点路由、线上 Analytics、API Key 设置或 Canvas Agent HTTP 客户端。
- 新增业务文件不超过 300 行；未经修改的 vendored 上游文件保留来源和哈希。
- Widget 只通过 `App.callServerTool()`、`sendMessage()` 和 display mode API 与宿主交互。

---

### Task 1: 建立 Widget workspace 和官方 MCP Apps client

**Files:**
- Create: `plugins/infinite-canvas/widget/package.json`
- Create: `plugins/infinite-canvas/widget/tsconfig.json`
- Create: `plugins/infinite-canvas/widget/index.html`
- Create: `plugins/infinite-canvas/widget/vite.config.ts`
- Modify: `plugins/infinite-canvas/widget/src/mcp-app.ts`
- Create: `plugins/infinite-canvas/widget/src/tool-result.ts`

**Interfaces:**
- Consumes: runtime tool names和 256 KiB host message ceiling。
- Produces: `connectApp()`、`callTool<T>()`、`sendAgentMessage()`、`requestDisplayMode()`。

- [ ] **Step 1: 固定 Widget dependencies**

`widget/package.json` 从 vendored 上游 `web/package.json` 复制全部实际 import 的生产依赖，
改为 exact versions，并增加：

```json
{
  "dependencies": {
    "@modelcontextprotocol/ext-apps": "1.7.5",
    "react": "19.2.5",
    "react-dom": "19.2.5"
  }
}
```

更新根 `pnpm-lock.yaml` 后运行 `pnpm --dir plugins/infinite-canvas install --lockfile-only`，
确认没有 tldraw、AGPL runtime 包或 git URL dependency。

- [ ] **Step 2: 实现官方 App 生命周期**

```ts
import { App } from "@modelcontextprotocol/ext-apps"

const app = new App(
  { name: "Infinite Canvas", version: "0.1.0" },
  { availableDisplayModes: ["inline", "fullscreen"] },
)

export async function connectApp(onLaunch: (canvasId: string) => void) {
  app.ontoolinput = ({ arguments: input }) =>
    onLaunch(validCanvasId(input?.canvasId) ? input.canvasId : "main")
  app.onteardown = async () => {
    stopSceneSync()
    return {}
  }
  await app.connect()
  return app
}
```

所有 handler 必须在 `connect()` 前注册；不访问 `window.parent` 或自定义 postMessage。

- [ ] **Step 3: 实现严格 tool result 解析**

`callTool<T>(name, args, parse)` 调用 `app.callServerTool({name, arguments: args})`；若
`isError`、缺少 JSON text/structuredContent 或 parse 失败，抛 `WidgetToolError(code,message)`。
错误 UI 只显示稳定 code 和安全消息，不渲染完整 payload。

- [ ] **Step 4: 实现 display mode 和消息接口**

`requestDisplayMode(mode)` 只接受 inline/fullscreen；`sendAgentMessage(text, image?)` 使用
`app.sendMessage({role:"user",content:[...]})`，正文包含 canvasId、revision、selection 和
用户意图，不包含 scene 全量或绝对路径。

### Task 2: 建立 remote scene store 和 revision 同步

**Files:**
- Create: `plugins/infinite-canvas/widget/src/scene-client.ts`
- Create: `plugins/infinite-canvas/widget/src/scene-store.ts`
- Create: `plugins/infinite-canvas/widget/src/operation-batcher.ts`
- Create: `plugins/infinite-canvas/widget/src/use-scene-sync.ts`
- Create: `plugins/infinite-canvas/widget/src/upstream/canvas-store-adapter.ts`

**Interfaces:**
- Produces: 与上游 `useCanvasStore` 相同的当前项目读取/更新 API；磁盘 authority 是 runtime。

- [ ] **Step 1: 实现 scene client**

```ts
export async function readScene(canvasId: string, sinceRevision?: number) {
  return callTool("get_infinite_canvas_state", { canvasId, sinceRevision }, parseSceneResult)
}
export async function applyOps(canvasId: string, baseRevision: number, operations: CanvasOperation[]) {
  return callTool("apply_infinite_canvas_ops", { canvasId, baseRevision, operations }, parseSceneResult)
}
```

- [ ] **Step 2: 实现 operation batcher**

用户 gesture 结束后把 operations 合并为一个有序 batch；同节点连续 update 合并 patch，add
后 remove 抵消，connection 保持引用顺序。batch 在前一请求结束前不并发发出；teardown
等待当前请求完成但不再启动新请求。

- [ ] **Step 3: 实现 revision conflict 重放**

收到 `revision_conflict` 时读取最新 scene，以 node/connection ID 为边界重新验证尚未确认的
operations；可重放项按原序提交一次，不可重放项保留在本地错误队列并提示用户，不做无限
重试或静默覆盖。

- [ ] **Step 4: 替换上游 Zustand persistence**

`canvas-store-adapter.ts` 暴露上游页面实际使用的 selector/actions，但只维护当前 canvas；
`createProject` 返回 launch canvasId，`updateProject` 转换为 operations，`deleteProjects` 在
Widget 中不可用。Vite exact alias：

```ts
"@/stores/canvas/use-canvas-store": resolve(widgetSrc, "upstream/canvas-store-adapter.ts")
```

- [ ] **Step 5: 实现可见性同步**

Widget 可见且无本地未提交 batch 时，每 1.6 秒调用 `get-state(sinceRevision)`；hidden、
teardown、runtime error 时停止。收到新 revision 后保留当前选择和 viewport，再替换 scene。

### Task 3: 替换媒体存储为 MCP 分块资产

**Files:**
- Create: `plugins/infinite-canvas/widget/src/asset-client.ts`
- Create: `plugins/infinite-canvas/widget/src/upstream/image-storage-adapter.ts`
- Create: `plugins/infinite-canvas/widget/src/upstream/file-storage-adapter.ts`
- Create: `plugins/infinite-canvas/widget/src/asset-url-cache.ts`

**Interfaces:**
- Produces: 上游 `saveImageFile/getImageUrl/deleteImageFile` 和媒体文件同义接口；scene 只保存 `asset://<sha256>`。

- [ ] **Step 1: 实现 128 KiB chunk uploader**

```ts
const CHUNK_BYTES = 128 * 1024
for (let offset = 0, index = 0; offset < file.size; offset += CHUNK_BYTES, index += 1) {
  const dataBase64 = await blobSliceToBase64(file.slice(offset, offset + CHUNK_BYTES))
  await writeAsset({ uploadId, chunkIndex: index, dataBase64, finalize: offset + CHUNK_BYTES >= file.size })
}
```

开始前拒绝空文件和 `file.size > 100 * 1024 * 1024`；取消时调用空 finalize/cancel 分支清理
临时文件。UI 显示累计字节，不按块刷 toast。

- [ ] **Step 2: 实现按需读取和 object URL cache**

`asset://sha` 首次显示时分块 `read-asset`，合成 Blob 并生成 object URL；引用计数归零或
teardown 时 `URL.revokeObjectURL()`。cache key 包含 sha 和 MIME，不持久化 blob。

- [ ] **Step 3: 适配上游 image/file storage**

两个 adapter 保持上游函数签名，内部只调用 asset client。删除节点不立即删除内容寻址资产；
孤儿清理由 runtime verifier/维护工具在无 scene 引用时执行，避免跨画布误删。

- [ ] **Step 4: 验证 bridge 上限**

构造 128 KiB 随机 chunk，序列化完整 tools/call JSON；Expected: UTF-8 byte length < 256 KiB。
构造 129 MiB File 元数据；Expected: 上传前返回 `asset_too_large`，没有 tools/call。

### Task 4: 建立专用 Widget shell 并裁剪站点能力

**Files:**
- Create: `plugins/infinite-canvas/widget/src/main.tsx`
- Create: `plugins/infinite-canvas/widget/src/widget-shell.tsx`
- Create: `plugins/infinite-canvas/widget/src/widget-toolbar.tsx`
- Create: `plugins/infinite-canvas/widget/src/widget-error-boundary.tsx`
- Create: `plugins/infinite-canvas/widget/src/widget.css`
- Create: `plugins/infinite-canvas/widget/src/upstream/analytics-noop.ts`
- Create: `plugins/infinite-canvas/widget/src/upstream/config-store-noop.ts`

**Interfaces:**
- Consumes: scene store、asset adapters、vendored canvas components。
- Produces: 一个 root React tree，无 BrowserRouter 和站点导航。

- [ ] **Step 1: 组合 WidgetCanvas**

`WidgetShell` 在 connect/initial scene 期间显示稳定 loading；ready 后挂载上游 InfiniteCanvas、
CanvasNode、CanvasConnections、CanvasToolbar、CanvasSidePanel 和 MiniMap。每个新增组件只负责
一个状态所有者，文件保持 ≤300 行。

- [ ] **Step 2: 裁剪不可达站点模块**

Widget entry 不 import `router.tsx`、Home/Image/Video/Assets/Prompts/Config 页面、Analytics、
LocalAgentPanel、agent-url-bootstrap、app-sync 和线上插件 registry。Vite alias analytics/config
为 no-op；build metafile 必须证明这些模块未进入 bundle。

- [ ] **Step 3: 接入 inline/fullscreen**

工具栏 full-screen 按钮调用官方 `requestDisplayMode()`；监听 `onhostcontextchanged` 更新布局，
不卸载 scene store。Escape 由宿主处理，Widget 不创建第二个 root 或 app instance。

- [ ] **Step 4: 建立确定错误态**

覆盖 connect 失败、runtime unavailable、scene corrupted、revision conflict、asset missing、
upload rejected 和 teardown。错误态提供重试当前动作，不跳转线上页面、不自动清空 scene。

### Task 5: 将 HTML、Markdown、SVG 和官方节点插件内嵌

**Files:**
- Create: `plugins/infinite-canvas/widget/src/plugins/register-builtins.ts`
- Modify: `plugins/infinite-canvas/vendor/infinite-canvas-web/canvas-plugins/html/src/index.tsx`
- Modify: `plugins/infinite-canvas/vendor/infinite-canvas-web/canvas-plugins/markdown/src/index.tsx`
- Modify: `plugins/infinite-canvas/vendor/infinite-canvas-web/canvas-plugins/svg/src/index.tsx`

**Interfaces:**
- Produces: 无远程 URL 的内建 node definitions。

- [ ] **Step 1: 静态注册三个节点插件**

把插件 `definePlugin()` 输出编译进 Widget，启动时调用 `registerBuiltinPlugins()`；不读
`/plugins/index.json`，不允许 URL 安装或远程更新。

- [ ] **Step 2: 收紧 HTML sandbox**

HTML preview iframe 使用 `sandbox=""`，CSP 为 `default-src 'none'; img-src data: blob:; style-src 'unsafe-inline'`；
编辑器文本写 scene，预览 DOM 不获得脚本、表单、导航、下载或网络权限。

- [ ] **Step 3: 验证节点 round-trip**

分别创建 HTML、Markdown、SVG，保存、重读、修改和删除；Expected: scene 只有 node metadata，
无 object URL、绝对路径、外部脚本或远程插件地址。

### Task 6: 构建单 HTML 并完成 Widget 门禁

**Files:**
- Modify: `plugins/infinite-canvas/scripts/build.mjs`
- Modify: `plugins/infinite-canvas/scripts/verify.mjs`
- Create: `plugins/infinite-canvas/widget/dist/infinite-canvas-widget.html`

**Interfaces:**
- Produces: runtime `resources/read` 可直接返回的自包含 HTML。

- [ ] **Step 1: 生成 Vite metafile 和单 HTML**

构建使用相对 base；把 JS/CSS/SVG/font 全部内联，拒绝剩余 `<script src>`、`<link href>`、
HTTP(S) URL 和动态 import chunk。保留 MCP Apps 官方 client，不内联 source map。

- [ ] **Step 2: 扩展 verifier**

检查 HTML <8 MiB、CSP 无 network/frame domain、bundle 无 `canvas.best`/LocalAgent/tldraw、
所有 manifest app tools 在 runtime schemas 中、HTML/Markdown/SVG 已注册。

- [ ] **Step 3: 运行 Widget 门禁**

Run:

```powershell
pnpm --dir plugins/infinite-canvas typecheck
pnpm --dir plugins/infinite-canvas build
pnpm --dir plugins/infinite-canvas verify
git diff --check
```

Expected: exit 0；Widget size < 8 MiB、0 external URLs、0 tldraw、14+ upstream canvas modules
进入 bundle，站点/Agent HTTP 模块为 0。

- [ ] **Step 4: 提交 Widget**

```powershell
git add -- plugins/infinite-canvas
git commit -m "feat(plugin): 接入 Infinite Canvas 原生画布"
```
