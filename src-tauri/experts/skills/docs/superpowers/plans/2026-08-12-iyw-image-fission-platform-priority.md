# IYW 图片分身生图平台选择 Implementation Plan

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `fission-generate` send to one platform by default, prefer platform 4, and only send all platforms when the caller explicitly requests comparison.

**Architecture:** Keep live model configuration parsing and the existing batch/task lifecycle intact. Add one pure selection function in `iyw_fission_core.py`; both live requests and dry-runs use it, while the CLI exposes an explicit `--compare-platforms` flag that defaults to false.

**Tech Stack:** Python 3.10+, `argparse`, existing async HTTP clients, pytest.

## Global Constraints

- Use only the existing microModel batch route and current authentication behavior.
- Default mode selects exactly one supported live fission platform.
- Platform `4` is preferred; when absent, fall back to the first supported platform in live configuration order.
- Comparison mode is opt-in, selects every supported live platform, and places platform `4` first when present.
- Duplicate, empty, and unsupported live fission configurations remain configuration errors.
- Dry-run must not fetch live configuration or credentials and must use the same selection semantics against built-in templates.
- Do not commit or alter unrelated dirty-worktree files.

---

### Task 1: Add platform selection tests

**Files:**
- Create: `tests/test_iyw_fission.py`

**Interfaces:**
- Tests import `_configured_model_payloads`, `_select_fission_models`, and `DEFAULT_FISSION_MODELS` from `iyw_fission_core`.
- Tests parse `fission-generate` with `iyw_commerce.build_parser`.

- [x] **Step 1: Write the failing unit tests**

```python
def test_default_selection_prefers_platform_four():
    payloads = _select_fission_models(_options("1", "4", "8"))
    assert [item["platform"] for item in payloads] == ["4"]


def test_default_selection_falls_back_to_first_live_platform():
    payloads = _select_fission_models(_options("8", "1"))
    assert [item["platform"] for item in payloads] == ["8"]


def test_comparison_selection_puts_platform_four_first():
    payloads = _select_fission_models(_options("8", "1", "4"), compare_platforms=True)
    assert [item["platform"] for item in payloads] == ["4", "8", "1"]


def test_comparison_selection_preserves_order_when_platform_four_missing():
    payloads = _select_fission_models(_options("8", "1"), compare_platforms=True)
    assert [item["platform"] for item in payloads] == ["8", "1"]


@pytest.mark.parametrize("options, message", [
    ([{"label": "分身未知", "value": "999"}], "unsupported live fission configuration"),
    ([], "no supported fission models are available"),
    ([{"label": "分身 A", "value": "4"}, {"label": "分身 B", "value": "4"}], "duplicate"),
])
def test_invalid_live_configuration_is_rejected(options, message):
    with pytest.raises(IywError, match=message):
        _select_fission_models(options)
```

Add a small `_options(*platforms)` helper that emits `{"label": f"分身 {platform}", "value": platform}` dictionaries, and add a CLI parser assertion that `fission-generate --prompt "x"` has `compare_platforms is False` while `--compare-platforms` sets it to true.

- [x] **Step 2: Run the focused tests to verify they fail**

Run: `uv run --with pytest --no-project python -m pytest tests/test_iyw_fission.py -q`

Expected: FAIL because the selection function and CLI flag do not exist yet.

### Task 2: Implement shared platform selection

**Files:**
- Modify: `iyw-image-workflows/scripts/iyw_fission_core.py:88-160,239-258`

**Interfaces:**
- Add `_select_fission_models(options: list[dict[str, Any]], *, compare_platforms: bool = False) -> list[dict[str, Any]]`.
- Extend `create_fission_tasks(..., compare_platforms: bool = False)` and `generate_fission_images(..., compare_platforms: bool = False)`.

- [x] **Step 1: Implement the pure selection function**

Keep `_configured_model_payloads` as the validator and add:

```python
PREFERRED_FISSION_PLATFORM = "4"


def _select_fission_models(
    options: list[dict[str, Any]], *, compare_platforms: bool = False
) -> list[dict[str, Any]]:
    payloads = _configured_model_payloads(options)
    ordered = sorted(
        payloads,
        key=lambda item: item["platform"] != PREFERRED_FISSION_PLATFORM,
    )
    if compare_platforms:
        return ordered
    return [ordered[0]]
```

The stable sort keeps live order for all non-preferred platforms; selecting `ordered[0]` naturally falls back to the first live platform when platform 4 is absent.

- [x] **Step 2: Thread the flag through live generation**

Use `_select_fission_models(options, compare_platforms=compare_platforms)` in `create_fission_tasks`, and pass the same keyword from `generate_fission_images` to `create_fission_tasks`.

- [x] **Step 3: Apply the same semantics to dry-run**

In the dry-run branch, call `_select_fission_models(list(DEFAULT_FISSION_MODELS), compare_platforms=compare_platforms)` only after converting built-in templates to option-shaped records, or factor a small helper that selects from already validated payloads. The resulting `models` must contain only platform 4 by default and all built-in platforms in comparison mode.

- [x] **Step 4: Run the focused unit tests**

Run: `uv run --with pytest --no-project python -m pytest tests/test_iyw_fission.py -q`

Expected: PASS.

### Task 3: Expose and document comparison mode

**Files:**
- Modify: `iyw-image-workflows/scripts/iyw_commerce.py:101-121,178-186`
- Modify: `iyw-image-workflows/SKILL.md:15-23,144-177`
- Modify: `iyw-image-workflows/references/fission-generation.md:1-42`
- Modify: `iyw-image-workflows/agents/openai.yaml:4`
- Modify: `tests/test_iyw_image_skill_docs.py`

**Interfaces:**
- CLI flag: `fission-generate --compare-platforms`.
- `run_command` passes `args.compare_platforms` into `generate_fission_images`.

- [x] **Step 1: Add the argparse flag and dispatch wiring**

Add `fission_generate.add_argument("--compare-platforms", action="store_true", help="compare all configured fission platforms")`, and pass `compare_platforms=args.compare_platforms` to the core function. Keep the flag absent by default.

- [x] **Step 2: Update the skill contract**

State that normal fission generation sends one image to platform 4, falls back to the first supported configured platform if 4 is unavailable, and requires `--compare-platforms` only for an explicit multi-platform comparison request. State that comparison order puts 4 first.

- [x] **Step 3: Update the reference and agent prompt**

Update the command description and example to show the default single-platform command plus a separate explicit comparison example. Add the same rule to the agent default prompt without exposing internal model details.

- [x] **Step 4: Extend documentation tests**

Assert the Skill and agent prompt contain the default single-platform rule, explicit comparison flag, platform-four priority, and fallback wording.

- [x] **Step 5: Run documentation tests**

Run: `uv run --with pytest --no-project python -m pytest tests/test_iyw_image_skill_docs.py -q`

Expected: PASS.

### Task 4: Run final targeted verification and clean up

**Files:**
- No new files; inspect only touched files and test outputs.

- [x] **Step 1: Run all directly related tests**

Run: `uv run --with pytest --no-project python -m pytest tests/test_iyw_fission.py tests/test_iyw_image_skill_docs.py tests/test_iyw_image_tools.py tests/test_iyw_search.py -q`

Expected: PASS with no failures.

- [x] **Step 2: Run static checks on touched Python files**

Run: `uv run --no-project python -m compileall iyw-image-workflows/scripts tests/test_iyw_fission.py`

Expected: exit code 0.

- [x] **Step 3: Inspect the diff and temporary files**

Run: `git diff --check; git status --short`

Expected: no whitespace errors; only the approved design/plan and implementation files are newly changed, with no generated test artifacts.

- [x] **Step 4: Do not commit without confirmation**

Because repository guidance requires confirmation for Git history operations, report the changes and verification without creating a commit.
