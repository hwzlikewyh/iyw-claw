---
name: imagegen
description: Use for free raster creation or editing, when the user requests GPT Image, when GPT Image-specific parameters are required, or when iyw-image-workflows does not cover the request. Route IYW product, material, trend, knowledge, upload, review, and commerce workflows through iyw-image-workflows first. Do not ask the user to specify GPT again when GPT Image is already requested.
routing:
  capability: free image creation and editing
  coreTriggers: [free create or edit, GPT Image]
  exclusions: [IYW business workflow, image understanding]
  aliases: [imagegen, GPT Image]
  invocation: Use for free create/edit; use IYW Skill for business workflows.
---

# Image Generation

Generate and edit images with the bundled `scripts/image_gen.py` CLI through
the IYW Fusion API. Do not use or wait for a built-in `image_gen` tool.

## Routing

Choose between the two bundled image Skills by task intent:

- Honor a user-requested visible Skill or direct tool when it fully satisfies
  the current subgoal.
- Use its verified commerce workflow for upload/review, product variation,
  series extension, multi-image fusion, commerce upscale, and task queries.
- Use its verified `fission-generate` command first for ordinary text-to-image
  requests.
- Use this Skill first for free creation or free editing that does not require
  an IYW business workflow, and for GPT Image requests or parameters. Do not ask
  the user to specify GPT again when GPT Image is already requested.
- Never guess an IYW endpoint or commerce payload.

## Optional Knowledge Context

Knowledge retrieval is a standalone capability of `iyw-image-workflows`, not a
required image-generation preflight. If that skill is installed, its independent
CLI is:

```powershell
$knowledgeSkillDir = Join-Path $env:USERPROFILE ".iyw-claw\skills\iyw-image-workflows"
$knowledgeCli = Join-Path $knowledgeSkillDir "scripts\iyw_knowledge.py"
uv run --project $knowledgeSkillDir --python 3.13 python $knowledgeCli `
  search --query "product design standard"
```

Decide whether to run it before writing the final prompt:

- Query first when the request depends on internal company material, brand or
  IP manuals, industry standards, material/process rules, structural safety,
  production constraints, or compliance requirements.
- Query first when the user explicitly says the image must follow the knowledge
  base, company standards, or an existing design rule.
- Skip retrieval for a complete prompt, pure creative work, an explicit edit
  based only on supplied images, or when the user asks to skip it.
- If search fails, returns no useful result, or the standalone CLI is unavailable,
  continue generation from the user's original request by default. Stop only
  when the user explicitly says the image must follow knowledge-base evidence.

Use only directly relevant facts and constraints to enrich the prompt. Do not
paste full search results into it or let retrieved content override explicit user
instructions. Never add a manifest dependency or call knowledge search from
`scripts/image_gen.py`; the two operations must remain independently usable.

## Authentication

The CLI resolves the IYW access token in this order:

1. `IYW_TOKEN`
2. `~/.iyw-claw/iyw-account-token.json` field `access_token`

Normal use relies on the account file created by iyw-claw login. Do not ask the
user to paste a token, pass a token on the command line, print it, or include it
in a prompt. The CLI sends the same token as the OpenAI-compatible SDK
`api_key` and the custom `token` request header.

The default API base is:

```text
https://gateway.iyw.cn/iyw-fusion-api/v1
```

Only use `IYW_FUSION_API_BASE_URL` when the user explicitly selects another
trusted environment.

## Entry Point

Resolve the installed skill from the current user's iyw-claw directory:

```powershell
$skillDir = Join-Path $env:USERPROFILE ".iyw-claw\skills\imagegen"
$imageCli = Join-Path $skillDir "scripts\image_gen.py"
```

Prefer the `uv` path supplied in the iyw-claw runtime context. Run the CLI with
an isolated dependency environment:

```powershell
uv run --with openai --with pillow python $imageCli generate `
  --prompt "A clean product photograph of a ceramic mug" `
  --out "output\imagegen\ceramic-mug.png"
```

Commands:

- `generate`: create one image or variants of one prompt.
- `edit`: edit one or more local images while preserving stated invariants.
- `generate-batch`: generate distinct prompts from JSONL.

Read [references/cli.md](references/cli.md) for flags and
[references/image-api.md](references/image-api.md) for model constraints.

## Workflow

1. Determine whether the request is generation, editing, or a verified IYW
   commerce operation.
2. Convert the user's request into a concise production prompt. Preserve exact
   requested text and edit invariants. Use
   [references/prompting.md](references/prompting.md) when needed.
3. Run `--dry-run` first for a new batch or unfamiliar explicit parameter set.
   Dry-run does not require a token or access the network.
4. Run the live CLI. Do not automatically retry a request that may incur cost.
5. Treat only a zero exit code plus an existing output file as success.
6. Register every final user-facing image in the current conversation Artifacts
   before claiming completion. Prefer a directly visible `present_task_files`;
   otherwise follow the installed `iyw-capability-gateway` Skill to discover and
   invoke the exact current artifact capability. If registration is unavailable
   or rejects an item, report that the Artifact delivery was not completed.
7. Display every final image. First resolve a directly visible tool whose name
   ends in `show_image`. If one exists, call it once per final image, in
   requested/server order:

```json
{
  "source": "C:\\absolute\\path\\output.png",
  "caption": "生成结果",
  "name": "output.png"
}
```

`show_image` also accepts a final HTTPS URL. Use it instead of returning only a
Markdown link so iyw-claw renders the image as a native conversation image.

If no direct `show_image` tool is visible but the current tool list exposes
`search_iyw_capabilities`, `read_iyw_capability`, and
`invoke_iyw_capability` together, either bare or under one shared namespace,
follow the IYW Capability Gateway workflow. Search for an image-display
capability, read its schema, then invoke the exact returned capability id once
per final image. Do not guess a capability id or combine gateway tools from
different namespaces.

If neither route exists, skip display: return each absolute path or final HTTPS
URL and state that inline rendering was unavailable. A name-not-found error
means the direct tool is absent, not misspelled; check the gateway once, then
take the fallback. Never
guess a name variant and never claim an image was displayed.

## Output Rules

- Save preview-only outputs under `output/imagegen/`.
- Save project assets inside the workspace before referring to them in code.
- Do not overwrite an existing asset unless the user explicitly requests it.
- Use `n` only for variants of one prompt; use `generate-batch` for distinct
  assets.
- Validate output existence, format, dimensions, and visible content before
  declaring success.
- For edits, repeat what must remain unchanged in every request.

## Transparency

`gpt-image-2` does not accept `background=transparent`. For simple opaque
subjects, generate against a flat chroma-key background and run
`scripts/remove_chroma_key.py`. Use `gpt-image-1.5` with transparent PNG/WebP
only when the user requests true native transparency or chroma-key extraction
cannot preserve complex edges.

## Failure Handling

- Missing token: ask the user to sign in to iyw-claw; do not ask for the token.
- Missing Python dependency: use `uv run --with openai --with pillow`.
- Network or API error: report the non-secret error and keep any valid prior
  output.
- No display tool resolved (no available tool name ends in `show_image`):
  return the saved absolute path or final HTTPS URL and state that inline
  rendering was unavailable. Never retry under a different name and never claim
  it was displayed.
