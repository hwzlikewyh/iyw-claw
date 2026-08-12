# IYW 图片趋势路由与成组版式 Implementation Plan

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route trend/theme image requests through series extension when a reference image exists, keep grouped-layout generation to one task first, and provide a deterministic local layout composer for the final fallback.

**Architecture:** Agent-facing Skill instructions own intent routing and the bounded `extend` to `variation` fallback. The existing fixed-tool validators force `batchSize: 1`, while a new dependency-light module owns local Pillow composition and `iyw_commerce.py` exposes it before any API client or token is initialized.

**Tech Stack:** Python 3.10+, argparse, Pillow, pytest, uv.

## Global Constraints

- Trend or theme plus one checked reference image uses `tool extend` first.
- No reference image continues to use `fission-generate`.
- Explicit specialized actions remain higher priority than trend routing.
- An explicit `extend` failure may create exactly one `variation` fallback; uncertain paid-task creation is queried by original task ID and is not treated as a failure.
- Grouped layouts use one `variation` or `extend` task with `batchSize: 1` before any split generation.
- Split generation and local composition occur only after an explicit task failure or a visual check confirms that the output does not match the requested layout.
- When visual inspection is unavailable, keep the single-task result and do not create more paid tasks.
- Do not guess layout dimensions, item count, or ordering.
- Preserve the current uncommitted platform-four changes and unrelated dirty-worktree files.
- Do not commit, push, or alter Git history without separate confirmation.

---

### Task 1: Add local composition tests and dependency

**Files:**
- Create: `tests/test_iyw_layout.py`
- Modify: `iyw-image-workflows/pyproject.toml`
- Modify: `iyw-image-workflows/uv.lock`

**Interfaces:**
- Tests import `compose_layout(images: list[Path], rows: int, columns: int, out: Path, *, gap: int = 0, background: str = "#FFFFFF", force: bool = False) -> dict[str, object]` from `iyw_layout`.
- The returned object exposes `out`, `width`, `height`, `rows`, `columns`, and `count`.

- [x] **Step 1: Add failing tests**

Create colored Pillow fixtures under pytest `tmp_path`. Test a 2x2 layout with mixed dimensions, a 1x4 order check, gap and canvas dimensions, wrong image count, invalid background, unsupported output suffix, existing output without `force`, and successful overwrite with `force`.

- [x] **Step 2: Verify tests initially fail**

Run: `uv run --with pytest --with pillow --no-project python -m pytest tests/test_iyw_layout.py -q`

Expected: collection fails because `iyw_layout` does not exist.

- [x] **Step 3: Add Pillow dependency and refresh lock**

Set `dependencies = ["pillow>=11.0.0"]` in `pyproject.toml`, then run `uv lock --project iyw-image-workflows`.

Expected: `uv.lock` contains the Pillow package and the project dependency edge.

### Task 2: Implement deterministic local layout composition

**Files:**
- Create: `iyw-image-workflows/scripts/iyw_layout.py`
- Test: `tests/test_iyw_layout.py`

**Interfaces:**
- Produce the `compose_layout(...)` function defined in Task 1.
- Raise `IywError` with `invalid_input` for invalid dimensions, count, paths, formats, colors, and overwrite attempts.

- [x] **Step 1: Validate every input before output creation**

Require positive rows/columns, nonnegative gap, exact `rows * columns` image count, existing supported input files, supported output suffix, an existing output parent directory, a six-digit hex background, and either a missing output or `force=True`.

- [x] **Step 2: Decode and normalize images**

Use `ImageOps.exif_transpose`, fully load each image, convert to RGBA, and calculate the cell as maximum input width by maximum input height. Reject corrupt or unsupported images as `invalid_input`.

- [x] **Step 3: Compose in row-major order**

Create an RGB canvas using the requested background. Use `ImageOps.contain(..., Image.Resampling.LANCZOS)` to preserve aspect ratio, center each image in its cell, composite alpha onto the background, and place cells in input order with the configured gap.

- [x] **Step 4: Save atomically**

Save to a uniquely named temporary file in the output directory with an explicit Pillow format, then replace the target. Delete the temporary file on a save failure. Return the normalized output metadata.

- [x] **Step 5: Run composition tests**

Run: `uv run --with pytest --with pillow --no-project python -m pytest tests/test_iyw_layout.py -q`

Expected: all layout tests pass.

### Task 3: Expose compose-layout without API initialization

**Files:**
- Modify: `iyw-image-workflows/scripts/iyw_commerce.py`
- Modify: `tests/test_iyw_layout.py`

**Interfaces:**
- CLI: `compose-layout --image PATH... --rows N --columns N --out PATH [--gap N] [--background #RRGGBB] [--force]`.
- `run_command` dispatches `compose-layout` before calling `_client(args)`.

- [x] **Step 1: Add a failing CLI test**

Parse and run `compose-layout` with four fixture images while monkeypatching `_client` to raise if called. Assert the command succeeds, returns output metadata, and writes a readable image.

- [x] **Step 2: Add parser and early dispatch**

Import `compose_layout`, add the local subcommand without connection arguments, and return its result before `client = _client(args)`. This command must not accept base URL, token, dry-run, polling, or progress flags.

- [x] **Step 3: Run CLI composition tests**

Run: `uv run --with pytest --with pillow --no-project python -m pytest tests/test_iyw_layout.py -q`

Expected: all layout and CLI tests pass.

### Task 4: Force single-task tool payloads

**Files:**
- Modify: `iyw-image-workflows/scripts/iyw_tool_core.py`
- Modify: `tests/test_iyw_image_tools.py`

**Interfaces:**
- `validate_tool_payload("variation", payload)` and `validate_tool_payload("extend", payload)` always produce `payload["batchSize"] == 1`.

- [x] **Step 1: Add failing validator tests**

Assert both aliases set missing `batchSize` to 1 and overwrite caller-supplied values greater than 1 with 1.

- [x] **Step 2: Implement the fixed value**

Set `payload["batchSize"] = 1` inside `_validate_variation` and `_validate_extend` after the image validation and before returning.

- [x] **Step 3: Run tool tests**

Run: `uv run --with pytest --with pillow --no-project python -m pytest tests/test_iyw_image_tools.py -q`

Expected: all fixed-tool tests pass.

### Task 5: Update routing and fallback contracts

**Files:**
- Modify: `iyw-image-workflows/SKILL.md`
- Modify: `iyw-image-workflows/references/commerce-operations.md`
- Modify: `iyw-image-workflows/agents/openai.yaml`
- Modify: `tests/test_iyw_image_skill_docs.py`

**Interfaces:**
- Agent-visible rules cover trend/theme routing, no-image behavior, bounded fallback, one-task grouped layouts, visual-validation conditions, and `compose-layout`.

- [x] **Step 1: Add failing documentation tests**

Assert the Skill and default prompt state: reference image plus trend/theme uses `tool extend`; no reference uses `fission-generate`; explicit failure falls back once to `tool variation`; grouped layouts keep one task and `batchSize: 1`; missing visual inspection does not create more tasks; detailed fallback uses `compose-layout`.

- [x] **Step 2: Update the concise Skill routing rules**

Replace duplicated prose where necessary so `SKILL.md` remains at or below 300 lines. Link to `references/commerce-operations.md` for payload and composition details.

- [x] **Step 3: Update detailed operation reference**

Document the exact routing sequence, uncertain-creation behavior, grouped-layout decision gate, and all `compose-layout` arguments and output rules.

- [x] **Step 4: Synchronize agent metadata**

Extend the single-sentence default prompt with the routing and one-task-first requirements while keeping `$iyw-image-workflows` explicit.

- [x] **Step 5: Run documentation tests**

Run: `uv run --with pytest --with pillow --no-project python -m pytest tests/test_iyw_image_skill_docs.py -q`

Expected: all documentation tests pass and `SKILL.md` has no more than 300 lines.

### Task 6: Final verification and cleanup

**Files:**
- Inspect all files changed by this plan and the preceding platform-four implementation.

- [x] **Step 1: Run the complete related regression set**

Run: `uv run --with pytest --with pillow --no-project python -m pytest tests/test_iyw_layout.py tests/test_iyw_fission.py tests/test_iyw_image_skill_docs.py tests/test_iyw_image_tools.py tests/test_iyw_search.py -q`

Expected: all tests pass.

- [x] **Step 2: Validate the Skill**

Run with `PYTHONUTF8=1` and `PYTHONDONTWRITEBYTECODE=1`: `uv run --with pyyaml --with pillow --no-project python C:\Users\iyw\.codex\skills\.system\skill-creator\scripts\quick_validate.py iyw-image-workflows`

Expected: `Skill is valid!`.

- [x] **Step 3: Run real local CLI smoke tests**

Use temporary fixture images to execute 1x4 and 2x2 `compose-layout` commands through `iyw_commerce.py`, reopen the outputs with Pillow, and remove all fixtures afterward. No network request or token read is allowed.

- [x] **Step 4: Review diff and constraints**

Run `git diff --check`, confirm touched Python files are at most 300 lines, confirm `SKILL.md` is at most 300 lines, inspect CodeGraph impact, and verify no `__pycache__` or test artifacts remain.

- [x] **Step 5: Request independent review**

Request a read-only review of the combined uncommitted change, address Critical or Important findings, rerun affected checks, and do not commit or push.
