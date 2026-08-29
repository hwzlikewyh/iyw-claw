# Infinite Canvas Migration and Release Implementation Plan

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 非破坏迁移 Cowart 画布，完成许可证、artifact、真实安装客户端和 Fusion 发布门禁，使 Infinite Canvas 可被普通用户安装和使用。

**Architecture:** 迁移器直接解析 Cowart JSON records，不加载 tldraw 包；每个来源 page 写新 canvas 和机器可读报告。发布复用 Fusion Admin Upload 的 init → 预签名 PUT → complete → deterministic artifact 流程，验证通过前保持市场版本 disabled。

**Tech Stack:** Node.js/TypeScript、Plugin v2 verifier、Fusion Skill Admin Upload API、TOS、Windows iyw-claw installed client。

## Global Constraints

- 先完成 Runtime Foundation、Native Widget 和 Creative Parity。
- 执行主计划的全部 Global Constraints。
- 迁移绝不修改或删除 `canvas/pages/*/cowart-canvas.json` 和 page assets。
- 迁移代码和 artifact 不依赖 `@tldraw/*`；未知 record 进入报告，不静默丢弃。
- 外部 Fusion 写入、Git push/merge 和市场发布前再次取得用户明确确认。

---

### Task 1: 增加 Cowart migration capability 和 record parser

**Files:**
- Create: `plugins/infinite-canvas/runtime/src/migration/cowart-types.ts`
- Create: `plugins/infinite-canvas/runtime/src/migration/cowart-reader.ts`
- Create: `plugins/infinite-canvas/runtime/src/migration/cowart-mapper.ts`
- Create: `plugins/infinite-canvas/runtime/src/migration/cowart-report.ts`
- Modify: `plugins/infinite-canvas/runtime/src/contracts.ts`
- Modify: `plugins/infinite-canvas/runtime/src/tool-handlers.ts`
- Modify: `plugins/infinite-canvas/.iyw-plugin.json`
- Create: `plugins/infinite-canvas/contracts/migrate-cowart-canvas.schema.json`

**Interfaces:**
- Produces: `migrate_cowart_canvas({pageId,targetCanvasId?,dryRun?})` 和 report JSON。

- [ ] **Step 1: 定义无 tldraw 的宽松 record 类型**

```ts
export type CowartRecord = {
  id: string
  typeName?: string
  type?: string
  parentId?: string
  x?: number
  y?: number
  rotation?: number
  props?: Record<string, unknown>
  meta?: Record<string, unknown>
}
```

Reader 只接受 JSON object，snapshot records 总数和文件大小使用 verifier ceiling；单条非法
record 写报告后跳过，不让 prototype key 进入 mapper。

- [ ] **Step 2: 声明 migration schema**

输入 pageId 只允许已存在的 `canvas/pages/<pageId>` 直接子目录；targetCanvasId 默认
`cowart-<normalized-pageId>`；dryRun 默认 false。manifest 新增
`plugin.infinite-canvas.canvas.migrate-cowart.v1`，最终 contract count 为 10。

- [ ] **Step 3: 映射标准 shape**

映射表固定为：image/video → media node；text/note → text node；frame/group → group；geo →
annotation rect/ellipse/diamond；arrow/line/draw → annotation shape；bookmark/embed → HTML link
node。绑定箭头两端均指向已映射 node 时改为 CanvasConnection，否则保留 annotation。

- [ ] **Step 4: 映射 Cowart custom metadata**

`cowartAiImageHolder` → pending/error image config node；`cowartAiDraftHolder`/HTML draft → HTML
node并复制 page asset；`cowartAiSlides` → `iyw:slides`，按 page asset 顺序生成 pages；未知
`cowart*` meta 写 `unsupportedRecords`，包含 record ID/type/reason，不包含大 payload。

- [ ] **Step 5: 保存非破坏结果**

目标 canvas 已存在时返回 `migration_target_exists`，不合并。成功写 scene/assets 后再写
`canvas/infinite-canvas/migrations/<pageId>-<timestamp>.json`；report 包含 source SHA-256、
mapped/skipped counts、warnings、targetCanvasId。dryRun 不写任何文件。

### Task 2: 增加迁移 UI 和用户可检查报告

**Files:**
- Create: `plugins/infinite-canvas/widget/src/migration/cowart-migration-dialog.tsx`
- Create: `plugins/infinite-canvas/widget/src/migration/cowart-migration-result.tsx`
- Modify: `plugins/infinite-canvas/widget/src/widget-toolbar.tsx`
- Modify: `plugins/infinite-canvas/skills/infinite-canvas-open/SKILL.md`

**Interfaces:**
- Consumes: migration capability。
- Produces: 发现、dry-run、确认迁移、打开结果四步 UI。

- [ ] **Step 1: 只读发现 Cowart pages**

open Skill 在用户明确要求迁移时才搜索；普通打开不扫描旧目录。迁移对话框列出 page ID、
源文件时间和 dry-run summary，不自动选择全部。

- [ ] **Step 2: 要求显式迁移确认**

确认文案说明新建目标、不修改旧文件、未知 shapes 会进入报告。用户取消零写入；确认后逐页
串行调用，单页失败不阻止其它页，但最终清楚列出成功/失败。

- [ ] **Step 3: 展示并打开结果**

结果页显示 mapped/skipped/warnings 和报告相对路径；“打开”重新调用 render 指定
targetCanvasId，不复用旧 Cowart app instance。

### Task 3: 完成许可证、依赖和 artifact 审计

**Files:**
- Modify: `plugins/infinite-canvas/THIRD_PARTY_NOTICES.md`
- Create: `plugins/infinite-canvas/dist/license-report.json`
- Modify: `plugins/infinite-canvas/scripts/verify.mjs`
- Modify: `plugins/infinite-canvas/scripts/package.mjs`

**Interfaces:**
- Produces: 可审计 license report、source/artifact manifest、SHA-256 receipt。

- [ ] **Step 1: 生成生产依赖许可证清单**

Run: `pnpm --dir plugins/infinite-canvas licenses list --prod --json`

将 package/version/license/resolved integrity 写入排序后的 `license-report.json`。MIT、ISC、BSD、
Apache-2.0 可接受；AGPL/GPL/SSPL/BUSL/unknown 直接失败并停止发布，不能靠根 MIT 覆盖。

- [ ] **Step 2: 关闭上游元数据冲突**

报告明确：根源码和 Canvas Agent 是 MIT；错误的上游 `.codex-plugin` AGPL 文件未进入源码
构建和 artifact；我们分发的每个 vendored 文件均受根 MIT 或单独 notice 覆盖。若无法从
上游一手证据确认，版本保持 disabled，不发布。

- [ ] **Step 3: 验证 source 和 artifact 完整性**

Verifier 读取 Git tracked vendor/source 列表，确认 upstream.json commit、source SHA、dist
hash 和 ZIP manifest。检查 ≤512 files、≤50 MiB expanded、无 symlink、无 tldraw、无未知
license、无 `.codex-plugin`/`.mcp.json`、runtime entrypoint 和 10 schemas/resource 一致。

- [ ] **Step 4: 生成最终 0.1.0 artifact**

Run:

```powershell
pnpm --dir plugins/infinite-canvas typecheck
pnpm --dir plugins/infinite-canvas build
pnpm --dir plugins/infinite-canvas verify
pnpm --dir plugins/infinite-canvas package
```

Expected: deterministic ZIP 两次 SHA-256 相同；receipt 记录 size、SHA、10 tools、1 resource、
Widget size、license counts 和 upstream commit。

### Task 4: 增加 Fusion 可重放导入脚本

**Files (repository `F:/projects/iyw/iyw-fusion-api`):**
- Create: `scripts/import_infinite_canvas_plugin.mjs`
- Create: `docs/infinite-canvas-plugin-import.md`

**Interfaces:**
- Consumes: plugin ZIP/source directory、Fusion Admin Upload API。
- Produces: disabled v2 Skill Market version 和 ready deterministic artifact。

- [ ] **Step 1: 读取 Fusion 仓库规则和现有 Cowart importer**

重新读取 `AGENT.md`、`docs/relay-proxy-development-plan.md`、
`scripts/import_cowart_plugin.mjs` 和 skilladmin upload handlers；只复用已存在 API，不新增
endpoint/schema/DB migration。

- [ ] **Step 2: 实现 dry-run**

脚本参数为 `--source`、`--dry-run`、`--base-url`；本地先运行插件 verifier、读取 receipt、
校验 slug/version/manifest/tool/resource/license。未提供 `FUSION_BASE_URL` 或
`FUSION_ADMIN_TOKEN` 时强制 dry-run，绝不发送网络请求。

- [ ] **Step 3: 实现 init → PUT → complete**

复用 `/admin/api/skills/uploads/init`、预签名 upload URL、
`/admin/api/skills/uploads/complete`；complete 后轮询 status/detail，要求 version artifact
`ready`、size/SHA 一致、10 capability + 1 app + 1 runtime + 1 connector + 3 Skill 组件完整。

- [ ] **Step 4: 默认保持 disabled**

新建/更新记录必须 `disabled=true`；脚本没有 `--publish` 参数，不能在导入结束自动开放。
正式开放在真实客户端验收后单独执行 `set-disabled(false)`。

- [ ] **Step 5: 运行 importer dry-run**

Run: `node scripts/import_infinite_canvas_plugin.mjs --dry-run --source <plugin-root>`

Expected: exit 0；打印 manifest/version/file count/size/SHA/components；无 HTTP/TOS/DB 写入。

### Task 5: 在当前正式客户端做本地与隐藏市场纵向验证

**Files:**
- Create: `plugins/infinite-canvas/dist/validation-receipt.json`
- No source changes unless evidence proves a generic host defect。

- [ ] **Step 1: 建立可回滚验证环境**

记录当前 iyw-claw 版本、Git commit、Fusion环境、数据库备份路径和 workspace；不关闭用户
当前客户端，使用独立测试 workspace/conversation。插件目录和 runtime PID 在开始前记录。

- [ ] **Step 2: 验证安装/拒绝/批准**

从 hidden 管理详情安装 0.1.0；拒绝权限时无 runtime/app instance/文件写入；批准后仅当前
workspace/Agent available，同一会话重新 search/read/invoke 无需重启。

- [ ] **Step 3: 验证原生 Widget**

打开后首帧非空；inline/fullscreen 复用 instance；创建文本/图片/视频/音频/group/connection/
HTML/Markdown/SVG；刷新和历史恢复一致；用户与 Agent 交替编辑无覆盖。

- [ ] **Step 4: 验证 Agent 兼容矩阵**

逐一验证当前声明 MCP 的 Claude Code、Codex、Gemini、Cline、OpenCode、Hermes、CodeBuddy、
Kimi Code、Grok 和受信 Custom；OpenClaw 与 Pi 必须明确显示 unsupported，不宣称可用。
某个运行时无法安装时记录外部阻塞证据，不能用 Codex 单项成功代替全矩阵结论。

- [ ] **Step 5: 验证创作流程**

执行图片生成、显式重试、标注编辑、HTML 生成/编辑、Slides 生成/演示/导出；验证 402/取消/
部分成功保留 prompt 和已有结果，不自动 replay。

- [ ] **Step 6: 验证迁移与生命周期**

迁移至少一个包含 image/text/arrow/HTML/Slides 的 Cowart fixture，检查报告和原文件 SHA
不变。随后验证升级、permission digest 变化、禁用、卸载和退出；无残留 runtime/lease，
`canvas/infinite-canvas` 数据仍存在。

- [ ] **Step 7: 写验证 receipt**

记录每项 pass/fail/evidence path、客户端版本、artifact SHA、plugin version、runtime PID
回收、workspace data hash；不记录 token、绝对用户文件内容或画布正文。

### Task 6: 提交、推送、合并和 Fusion 开放

**Files:**
- All task files in iyw-claw and iyw-fusion-api repositories。

- [ ] **Step 1: 完成提交前审计**

两个仓库分别检查 status、diff、staged diff、tracked vendor、生成产物、许可证、verifier 和
validation receipt；精确暂存任务文件，不包含其它脏改动。

- [ ] **Step 2: 取得外部变更确认**

向用户报告 commit 列表、验证结果、未验证平台、Fusion hidden ID/version 和风险；明确请求
push、merge main、上传/开放 Fusion 的授权。没有确认时停在本地提交和 hidden artifact。

- [ ] **Step 3: 推送并逐远程验证**

按用户确认的 remotes push feature branches，逐个 `git ls-remote` 核对 SHA；不 force-push。
合并 main 前 fetch 最新 main、检查冲突和并发变更，合并后再次运行 package/verifier。

- [ ] **Step 4: 开放 Fusion**

对精确 skill ID 调 `set-disabled(false)`；普通用户目录精确搜索 slug，执行首次安装和再次打开。
检查公开 detail/version/artifact/components 与 hidden 验收完全一致。

- [ ] **Step 5: 完成审计**

核对主分支 SHA、远程 SHA、Fusion ID/version/ready artifact、普通目录和已安装客户端；只有
主计划 Completion Gate 全部有证据时调用 goal complete。

- [ ] **Step 6: 清理临时产物**

仅移除本任务创建且已确认不再需要的 clone/build cache；保留 tracked source、artifact
receipt、数据库备份、migration report 和用户 workspace 数据。删除任何目录前解析并验证
绝对路径位于任务临时根目录。
