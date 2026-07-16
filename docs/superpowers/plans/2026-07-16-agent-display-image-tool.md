# Agent Display Image Tool Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an always-enabled `show_image` MCP tool that renders local, remote, Data URI, and raw Base64 images in the conversation with external source links.

**Architecture:** A focused Rust `image_tool` module loads, bounds, validates, and encodes one image, returning standard MCP image content plus a namespaced JSON metadata text item. Existing ACP image extraction carries the bytes; frontend metadata parsing distinguishes displayed images from generated images in both live and historical paths, and the shared image card opens file/URL sources externally.

**Tech Stack:** Rust/Tokio/Reqwest/Base64, MCP JSON-RPC, ACP tool-call events, React 19, TypeScript, next-intl, Tauri opener.

## Global Constraints

- `show_image` is enabled for every agent that supports injected MCP servers; no settings toggle.
- Sources: local path/file URI, HTTP/HTTPS, Data URI, raw Base64 with `mime_type`.
- Maximum decoded image size is 10 MiB; HTTP timeout is 15 seconds.
- Supported MIME types: PNG, JPEG, GIF, WebP, BMP, AVIF, SVG.
- Local paths open only on a local desktop; URLs open externally everywhere; Base64 has no source link.
- Keep every new production file under 300 lines and every function under 50 lines.
- Do not add permanent standalone test files. Keep only `src-tauri/scripts/release-contracts.test.mjs`.

---

### Task 1: MCP Image Loader And Tool Response

**Files:**
- Create: `src-tauri/src/acp/delegation/image_tool.rs`
- Modify: `src-tauri/src/acp/delegation/mod.rs`
- Modify: `src-tauri/src/acp/delegation/companion.rs`
- Modify: `src-tauri/src/acp/delegation/tool_schema.json`
- Modify: `src-tauri/src/bin_targets/iyw_claw_mcp.rs`
- Modify: `src-tauri/src/acp/connection.rs`

**Interfaces:**
- Produces: `image_tool::execute(arguments: Value, working_dir: PathBuf) -> Value`.
- Produces: MCP metadata text with `type = "iyw_claw_display_image"` followed by standard MCP image content.
- Consumes: session `working_dir` passed as `--working-dir` into `CompanionContext`.

- [ ] **Step 1: Add failing inline feature and loader tests**

Add Rust tests inside `companion.rs` and `image_tool.rs` that assert:

```rust
assert!(CompanionFeatures::parse(Some("images")).allows_tool("show_image"));
assert_eq!(result["content"][1]["type"], "image");
assert_eq!(result["content"][1]["mimeType"], "image/png");
assert_eq!(result["isError"], false);
```

Also cover Data URI, raw Base64 with MIME, local relative path, over-10-MiB rejection, and mismatched magic bytes.

- [ ] **Step 2: Run the tests and verify RED**

Run:

```powershell
cargo test --lib --no-default-features --features mcp-runtime,test-utils show_image -- --nocapture
```

Expected: compile/test failure because `images`, `show_image`, and `image_tool` do not exist.

- [ ] **Step 3: Implement the focused loader**

Create these core types and entry point:

```rust
pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Serialize)]
struct DisplayImageMetadata {
    r#type: &'static str,
    caption: Option<String>,
    name: String,
    source_kind: Option<&'static str>,
    source: Option<String>,
}

pub async fn execute(arguments: Value, working_dir: PathBuf) -> Value {
    match load_image(arguments, &working_dir).await {
        Ok(image) => image.success_result(),
        Err(error) => json!({
            "content": [{ "type": "text", "text": error.to_string() }],
            "isError": true,
        }),
    }
}
```

Split local path, URL, Data URI, raw Base64, MIME detection, size checking, and metadata rendering into functions under 50 lines.

- [ ] **Step 4: Register the schema and cancellable local call**

Add the tool schema:

```json
{
  "name": "show_image",
  "description": "Display one image directly in the iyw-claw conversation.",
  "inputSchema": {
    "type": "object",
    "required": ["source"],
    "properties": {
      "source": { "type": "string" },
      "mime_type": { "type": "string" },
      "caption": { "type": "string", "maxLength": 2000 },
      "name": { "type": "string", "maxLength": 255 }
    }
  }
}
```

Add `images: bool` to `CompanionFeatures`, map `show_image` to it, and add a `register_and_spawn_local` helper that races `image_tool::execute` against MCP cancellation without contacting the delegation broker.

- [ ] **Step 5: Make image tools always available with an explicit working directory**

Change `companion_features_arg` to include `images` unconditionally for MCP-capable sessions, pass:

```rust
"--working-dir", working_dir.to_string_lossy().as_ref()
```

and parse it into `CompanionContext.working_dir` in the sidecar binary.

- [ ] **Step 6: Verify GREEN and commit**

Run the filtered tests again and commit:

```bash
git add src-tauri/src/acp/delegation src-tauri/src/acp/connection.rs src-tauri/src/bin_targets/iyw_claw_mcp.rs
git commit -m "feat(mcp): 添加 Agent 图片展示工具"
```

---

### Task 2: Durable Display-Image Metadata

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/stores/conversation-runtime-store.ts`
- Modify: `src/lib/adapters/ai-elements-adapter.ts`
- Temporarily create then delete: `src/lib/display-image-metadata.test.ts`

**Interfaces:**
- Produces: `DisplayImageMetadata` and `parseDisplayImageMetadata(text)`.
- Produces: frontend-only `ContentBlock` variant `{ type: "display_image", ... }`.
- Produces: `AdaptedDisplayedImagePart` with `type: "displayed-image"`.

- [ ] **Step 1: Write a temporary failing metadata test**

Test valid metadata, unrelated tool text, malformed JSON, local paths, URLs, and Base64/no-source:

```ts
expect(parseDisplayImageMetadata(valid)).toEqual({
  caption: "Monthly revenue",
  name: "chart.png",
  sourceKind: "file",
  source: "C:\\workspace\\chart.png",
})
expect(parseDisplayImageMetadata("ordinary output")).toBeNull()
```

- [ ] **Step 2: Run the temporary test and verify RED**

Run `pnpm vitest run src/lib/display-image-metadata.test.ts` and expect an import/export failure.

- [ ] **Step 3: Implement metadata parsing and live conversion**

Export the parser from `ai-elements-adapter.ts` or a focused production helper. In the live reducer, when `ToolCallInfo.images` is non-empty and the content carries the metadata envelope, emit one `display_image` block per image; otherwise preserve current `image_generation` behavior.

- [ ] **Step 4: Implement historical conversion**

Extend `adaptImageToolResultParts` to accept `output_preview`, parse the same envelope, and return `displayed-image` parts. Ordinary image-bearing Read/PDF results continue returning `generated-image` parts unchanged.

- [ ] **Step 5: Verify GREEN, delete the temporary test, and commit**

Run the temporary test, remove it, run TypeScript/ESLint on the production files, then commit:

```bash
git add src/lib/types.ts src/stores/conversation-runtime-store.ts src/lib/adapters/ai-elements-adapter.ts
git commit -m "feat(chat): 贯通图片展示工具元数据"
```

---

### Task 3: Display Card And External Source Link

**Files:**
- Modify: `src/components/message/generated-images-block.tsx`
- Modify: `src/components/message/content-parts-renderer.tsx`
- Modify: `src/lib/platform.ts`
- Modify: `src/i18n/messages/{ar,de,en,es,fr,ja,ko,pt,zh-CN,zh-TW}.json`

**Interfaces:**
- Consumes: `AdaptedDisplayedImagePart`.
- Produces: shared card presentation `generated | displayed`.

- [ ] **Step 1: Generalize the image card**

Add a presentation prop and source metadata:

```ts
type ImagePresentation = "generated" | "displayed"

interface ImageSourceLink {
  kind: "file" | "url"
  target: string
  label: string
}
```

Keep preview/download behavior unchanged. For displayed images, render the caption and a compact source row with `ExternalLink`.

- [ ] **Step 2: Implement external-only opening**

Use existing platform APIs:

```ts
if (source.kind === "url") await openUrl(source.target)
if (source.kind === "file" && isLocalDesktop()) await openPath(source.target)
```

Never call internal workspace-tab APIs. Render remote/web local paths as non-clickable text.

- [ ] **Step 3: Route displayed parts and add all locale strings**

Render `displayed-image` with `presentation="displayed"`. Add localized title, source label, unavailable hint, and open failure text to all ten locale files.

- [ ] **Step 4: Format, lint, and commit**

Run targeted Prettier/ESLint and commit:

```bash
git add src/components/message src/lib/platform.ts src/i18n/messages
git commit -m "feat(chat): 显示 Agent 图片与外部来源链接"
```

---

### Task 4: End-To-End Verification

**Files:**
- No permanent new files.

**Interfaces:**
- Consumes: complete `show_image` tool and rendered conversation card.

- [ ] **Step 1: MCP JSON-RPC smoke test**

Build `iyw-claw-mcp`, start it with `--features images --working-dir <temp>`, send `initialize`, `tools/list`, and `tools/call` lines, and assert the response contains metadata plus a PNG MCP image item.

- [ ] **Step 2: Run project verification**

Run `pnpm build`, targeted ESLint/Prettier, desktop `cargo check --lib`, server/MCP check and clippy, and `node --test src-tauri/scripts/release-contracts.test.mjs`.

- [ ] **Step 3: Enforce the no-test-file requirement**

Scan for standalone tests, including `_tests.rs`; the only allowed result is `src-tauri/scripts/release-contracts.test.mjs`. `src-tauri/src/db/test_helpers.rs` remains production test scaffolding, not a standalone test suite.

- [ ] **Step 4: Visual verification**

Start the app/frontend, exercise a displayed-image fixture, and capture desktop and narrow screenshots. Confirm the image is nonblank, the source text does not overlap, URL/file source behavior matches platform mode, and the card remains stable while loading/failing.

- [ ] **Step 5: Final review and commit**

Review the complete diff, confirm working tree cleanliness, and commit any final integration corrections with `fix(chat): 完善图片展示工具集成`.
