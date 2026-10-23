use serde_json::{json, Value};

use super::iyw_image::limits::{
    MAX_BATCH_ATTEMPTS, MAX_BATCH_EXECUTIONS, MAX_BATCH_ITEMS, MAX_CONCURRENT_EXECUTIONS,
    MAX_FALLBACKS, MAX_IMAGE_INPUTS, MAX_REQUEST_COUNT,
};
use super::tool_identity::IMAGE_TOOL;

pub(super) fn tool() -> Value {
    json!({
        "name": IMAGE_TOOL,
        "description": concat!(
            "Generate, edit, or process images through an applicable platform operation or image model before installing image libraries or writing processing code. ",
            "A missing dedicated operation, keyword match, or detailed model description does not establish unsupported editing: evaluate prompt-driven platform edits or list_iyw_image_models. ",
            "Match the user's effect and actual inputs; verify technical constraints such as masks, transparency, bit depth, reference counts and dimensions against documentation. Broad capability flags do not guarantee these constraints. Do not invent stricter requirements or discard real ones. ",
            "Prefer applicable platform features for ordinary customer image tasks: micro-generate for text-only platform creation, variation for redesign, extend for series and mix for references. For complex tasks or freer creative control, use Fusion generate (images/generations) for text-only image-model creation, or explicit edit (images/edits) with source images and a selected catalog model. Use an explicit type for the chosen route; auto without images still selects Fusion generate. ",
            "Fusion parameters include quality, size, n, background, output_format, output_compression, response_format, moderation, input_fidelity, style and user, subject to selected-model support. For transparent output set parameters.background=transparent and use png/webp output; preserve this requirement in fallback candidates and select models documented to support transparency. Do not silently remove required options after an error. edit masks may use parameters.mask or an images item with role=mask and are sent as HTTPS mask.image_url references. The current relay supports complete JSON only, not image SSE streaming. ",
            "For generate/edit first call list_iyw_image_models, choose a model supporting the operation, and copy its opaque model_ref unchanged into parameters.model. No default model is selected. Prefer applicable platform features for customer image tasks. Choose image models directly for complex transformations, multiple visual constraints, freer creative control, or explicit user requests; platform incompatibility or confirmed failure also permits model editing. Documented incompatibility needs no failed trial. Reuse the catalog for the same task or batch. ",
            "Prioritize platform custom redesign (variation) for one-image changes, multi-image fusion (mix) for combining 2-10 references, and series extension (extend) for four-panel grids or series from one base image. Keep references and describe the composition in prompt. With no base, use generate. Match other specialized operations to their actual functions and limits. Prioritize platform ecommerce video via video when its contract fits, otherwise use the gateway Skill's documented ecommerce fetch contract. A complete prompt needs no extra director call. Built-in image-to-3d is disabled, including fetch/browser fallbacks. Platform types need no Fusion model lookup, and passing a catalog model to them does not select that Fusion model. ",
            "For variation/extend/mix, a user's platform model one/two/three means parameters.modelChannel=1/2/3. Preserve that selection and the platform route; never pick a Fusion catalog entry by list position. Only omit modelChannel when the user left the channel unspecified. Do not substitute another model in fallbacks when the user locked the model. ",
            "Recover by failure type: correct local path/parameter errors; after a confirmed route failure, reassess suitable service alternatives for the original goal. A failed auxiliary step does not establish a need for local implementation. The host retries acknowledged output download/persistence 502 failures at the same model up to three times with 2/4/8-second backoff, even without fallbacks; each retry resubmits generation and may incur charges. Other unchanged retries are not automatic; do not enumerate unrelated services. ",
            "For timeout, transport failure or a running/uncertain task, query the original task_id when available before any new submission; preserve uncertainty if unresolved. A backend error is not a tool-identity error and does not justify guessing capability IDs. ",
            "When required visual acceptance finds an inadequate completed image, prefer a targeted correction through a suitable service within task scope and charging authorization. Do not keep regenerating beyond scope. ",
            "Scripts, local image libraries and hand-drawn SVG/Canvas are fallback methods only with concrete evidence that both applicable platform and model routes are unavailable, documented as incompatible, or confirmed failed after bounded recovery. Explain the limitations first. ",
            "Copying/downloading inputs and embedding completed images are supporting actions. Redrawing, segmentation, structural inference and height remapping remain core image work even when called refinement, precision processing or post-processing. A belief that local algorithms are more precise is not fallback evidence. ",
            "User-facing replies, progress, questions and delivery descriptions use business display names only, or a generic image-processing name. Never disclose currency prices, model_ref, internal model/provider IDs or backend names, including in tables, code, error quotations or answers about routing and model choice. Only report points explicitly confirmed by the platform; never convert prices into points or invent a quote. Describe only execution confirmed by the result, using metadata.model_name when available. ",
            "Default timeouts: platform requests/polling 600 seconds, product-kit/a-plus HTTP 650, batch-center submit HTTP 120 (polling 600), Fusion 300. wait.timeoutSeconds overrides these, including above 600; prefer defaults or longer for slow tasks. ",
            "This tool has no image-generation capability_id: use its definition directly without capability search/read/invoke, separate upload, or another image Skill. ",
            "When the user needs multiple independent images, use requests; read its schema for task and concurrency limits. Each task may have completely different prompt, images, type, parameters, count, wait and fallbacks; do not copy one task's parameters over another. Larger sets use multiple bounded batches. count repeats one unchanged intent; never combine it with parameters.n or parameters.batchSize. Batches validate before generation, continue after runtime item failures, and return partial results in input order; top-level delivery.registerArtifact controls registration of successful URLs and defaults to false. ",
            "For generation/editing, prefer a small ordered chain of suitable candidates when multiple compatible routes exist within task authorization, rather than stopping after one confirmed route failure. A task may declare up to three ordered fallbacks with explicit type and complete parameters (plus optional wait); reuse its original prompt and all source images. Each execution stops at the first successful or partial result, a pending/canceled task, or uncertain submission. Only a confirmed terminal failure or a host-classified retryable response advances the chain; timeouts, transport/query failures and auth/input errors stop. Acknowledged output download/persistence 502 failures first receive the host's bounded same-model retries, then may advance a declared chain; generation may already have completed and incurred charges. Do not declare duplicate candidates or silently discard references. metadata.attempts preserves each outcome. ",
            "Image artifact registration defaults to off. Keep generated results unregistered while creating, comparing or selecting candidates; set delivery.registerArtifact=true only when the image itself is selected as a final file to give the user in this conversation. For images used to make another deliverable, leave registration off and register the completed deliverable with present_task_files after the task is complete. The top-level delivery option applies to every item in a batch. Deliver ordinary successful images using returned status, URLs and delivery metadata. Inspect visuals for requested review, comparison, visual acceptance or integration into a composed deliverable; a detailed prompt alone does not require review. Report partial or failed status honestly."
        ),
        "inputSchema": tool_input_schema()
    })
}

fn parameters_schema() -> Value {
    let mut schema = json!({
        "type": "object", "additionalProperties": true,
        "description": "Only fields supported by the selected operation. For generate, auto without images, or explicit edit, call list_iyw_image_models and set model to the exact selected model_ref: generate requires capabilities.image_generation=true, edit requires capabilities.image_editing=true and source images. Keep references in tool arguments; use business display names in replies and never disclose currency prices or actual model IDs. Match visual changes through the prompt; a missing same-named operation is not unsupported editing. Verify technical constraints such as input count, output format, size and masks against documentation; broad flags do not guarantee them. Do not invent options or stricter requirements, or drop real requirements. Prioritize applicable platform variation/extend/mix routes; choose image models for complex tasks, freer creative control, explicit requests, or platform limitations/unavailability/confirmed failure. Do not manufacture a failed trial. A missing/stale model_ref, raw model ID or display name used as model is rejected; no default model is selected. Other Fusion options must be supported by the model. variation, extend and mix need only prompt and images for a default result; their toolName is host-owned, while parameters.modelChannel preserves an explicit platform model selection (model one=1, model two=2, model three=3). Only an omitted channel defaults to 2. Never interpret a platform model label as a Fusion catalog position; parameters.model is rejected for these routes. Do not copy parameters across operations. Existing specialized parameters: free-imitation requires model=free and stats.width/height/strength; outpaint top/right/bottom/left in [0,1]; super-resolution upscale=2|4; split-layers model=extract_layers; enhance enhanceType=1|2 and integer model; convert inputFormat/outputFormat in png/jpg/jpeg/webp/gif/bmp; line-extraction model=realistic|canny, positive batch_size, stats.reference; video ratio, duration=4..15, mode=normal|hd; model-scene/background size and resolution=standard|4K. New native types: modify/seed-edit/micro-variation need images+prompt; blend 2-10 images, prompt optional; erase needs HTTPS mask and accepts prompt/payOrderNo; watermark-erase accepts target=text|watermark|text_watermark or a HTTPS mask; lineart accepts style; upscale/super-upscale accept upscale/op; vectorize/three-views/extract/detect-grid/build-extract-prompts need images; bleed-line needs size or bleed from the actual page; extract-color needs images; save-color needs colors; classify-intent needs text or keys; background-remove/check-image need one image; micro-upscale/micro-upscale-image need one image and known amount/device/payMethod/typeId; video-auto-director/video-remake-director need images+prompt and documented ratio/duration/mode; micro-generate needs prompt and optional images/modelChannel/ratio/batchSize/tool; scheme-generate needs an existing schemeId+prompt; faddish needs prompt and optional images; g-tools needs prompt and optional images/model; f-tools needs content and documented price/type, or toolName plus imageUrls from the current page. product-kit/a-plus need images and platform/market/language/contentType/resolution/productInfo/modules; a-plus also needs selectedPlan exactly from fetch_iyw_url product-kit/plan-a-plus. a-plus-edit needs one image+prompt and optional size/stats/jsonData; the host fixes variation/toolType=12/modelChannel=2/batchSize=1/isChange=1. Batch-center types batch-shape-fill, batch-generate, batch-watermark, batch-series-extend, batch-enhance, batch-upscale, batch-background-remove, batch-extract-pattern, batch-replace-scene, batch-mockup accept images and per-input batchSize (default 1). batch-generate/series-extend need prompt; watermark needs target=text|watermark|text_watermark; upscale scale=2..8 defaults 2; enhance defaults enhanceType=2/model=0; mockup requires mockupId. Remaining fields come from the selected batch tool. Use the gateway Skill's product-kit, batch-center and enum references for detailed contracts. Batch-center metadata.batch_id and metadata.query identify the status request; keep batchId distinct from taskId. Planning, copy, history, status, image-version saving and downloads all use fetch_iyw_url. Unknown field types/enums must come from an actual page contract. Data, colors, prompts and vector/video links are in metadata.result, including batch runs. Query existing tasks with fetch_iyw_url and metadata.query; do not repeat generation after polling errors.",
        "properties": {
            "model": {"type": ["string", "integer"], "description": "generate/edit: exact opaque model_ref selected from list_iyw_image_models. Platform operations retain their own documented model enum/type."},
            "modelChannel": {"type": "integer", "minimum": 0, "maximum": 4, "description": "Platform channel ID. For variation/extend/mix, preserve explicit selection: model one=1, model two=2, model three=3; omitted channel defaults to 2. This is not a Fusion catalog position or model_ref. Other operations follow their contracts; a-plus-edit requires 2."},
            "mask": {"type": ["string", "null"], "description": "edit: PNG mask as workspace path, HTTPS URL or Data URL; local data is uploaded first, then sent as a JSON mask.image_url HTTPS reference. Alternatively supply one images item with role=mask. Never provide both. Platform erase/watermark-erase retain their own HTTPS mask contract."},
            "strength": {"type": "number", "minimum": 0},
            "text": {"type": "string", "minLength": 1, "description": "classify-intent: current canvas instruction text."},
            "target": {"type": "string", "enum": ["text", "watermark", "text_watermark"], "description": "watermark-erase or batch-watermark target."},
            "selectedPlan": {"type": "object", "description": "a-plus: exact plan object returned by product-kit/plan-a-plus."},
            "modules": {"type": "array", "minItems": 1, "description": "product-kit/a-plus: selected module keys from the current product-kit workflow."},
            "upscale": {"type": "number", "minimum": 0, "description": "Operation-specific scale. super-resolution only accepts 2 or 4; other operations follow their own contract."},
            "colors": {"type": "array", "minItems": 1, "description": "save-color: use entries from extract-color output."},
            "keys": {"type": "array", "minItems": 1, "description": "classify-intent: known canvas context keys; the current page can instead send parameters.text."},
            "schemeId": {"type": ["string", "integer"], "description": "scheme-generate: existing design scheme ID from a business query."},
            "stats": {"type": "object", "description": "Operation-specific structure: line-extraction reference."},
            "models": {"type": "array", "items": {"type": "object"}, "description": "fission: confirmed model configuration objects, not display-name strings."}
        }
    });
    schema["properties"]
        .as_object_mut()
        .expect("image parameters")
        .extend(
            super::iyw_image::parameters_schema()
                .as_object()
                .expect("Fusion parameters")
                .clone(),
        );
    schema
}

fn tool_input_schema() -> Value {
    let mut schema = image_request_schema(false);
    schema["properties"]["delivery"] = delivery_schema();
    let single_properties = schema["properties"]
        .as_object()
        .expect("image request properties")
        .keys()
        .map(|name| {
            (
                name.clone(),
                json!({"$ref": format!("#/properties/{name}")}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    schema["properties"]["requests"] = json!({
        "type": "array", "minItems": 1, "maxItems": MAX_BATCH_ITEMS,
        "description": format!("Use for up to {MAX_BATCH_ITEMS} independent image tasks, with at most {MAX_CONCURRENT_EXECUTIONS} concurrent executions. Each item owns its prompt, images, type, parameters, count, wait and fallbacks; they may all differ. Sum count * (1 + fallback count) must not exceed {MAX_BATCH_EXECUTIONS} planned candidate executions; output 502 retries add up to 3 per candidate, at most {MAX_BATCH_ATTEMPTS} actual calls per batch. Split larger sets into batches. Do not mix requests with root type, prompt, images, parameters, count, wait or fallbacks."),
        "items": image_request_schema(true)
    });
    schema["oneOf"] = json!([
        {
            "$ref": "#/$defs/promptRequirements",
            "properties": single_properties,
            "additionalProperties": false
        },
        {
            "required": ["requests"],
            "properties": {
                "requests": {"$ref": "#/properties/requests"},
                "delivery": {"$ref": "#/properties/delivery"}
            },
            "additionalProperties": false
        }
    ]);
    schema["$defs"]["promptRequirements"] = json!({"anyOf": prompt_requirements()});
    schema
}

fn image_request_schema(include_id: bool) -> Value {
    let mut properties = json!({
        "type": image_type_schema(),
        "prompt": {"type": "string", "maxLength": 12000, "description": "Required non-blank text for creation and prompt-driven edits, including auto, fission, generate, edit, variation, extend, and mix. Put prompt directly in this tool's arguments (or each requests item), not only in assistant text or an extra arguments wrapper. State what to preserve/change and each reference image's role. Select specialized tools with type. Omit only for operations that do not require a prompt."},
        "images": image_sources_schema(),
        "parameters": parameters_schema(),
        "count": {
            "type": "integer",
            "minimum": 1,
            "maximum": MAX_REQUEST_COUNT,
            "default": 1,
            "description": "Repeat this same intent, including its optional recovery chain, as separate charged executions. Use requests for differing tasks. Do not combine with parameters.n or parameters.batchSize."
        },
        "fallbacks": fallbacks_schema(),
        "wait": wait_schema()
    });
    if include_id {
        // 批量项复用顶层字段，避免重复 schema 触发运行时有损压缩。
        for (name, value) in properties
            .as_object_mut()
            .expect("image request properties")
        {
            *value = json!({"$ref": format!("#/properties/{name}")});
        }
        properties["id"] = json!({"type": "string", "minLength": 1, "maxLength": 64});
    }
    let mut schema = json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false
    });
    if include_id {
        schema["$ref"] = json!("#/$defs/promptRequirements");
    }
    schema
}

fn fallbacks_schema() -> Value {
    let mut kind = image_type_schema();
    kind["enum"]
        .as_array_mut()
        .expect("image types")
        .retain(|value| value.as_str() != Some("auto"));
    kind.as_object_mut()
        .expect("image type object")
        .remove("default");
    kind["description"] =
        json!("Explicit compatible operation for this candidate; auto is not allowed.");
    json!({
        "type": "array", "minItems": 0, "maxItems": MAX_FALLBACKS,
        "description": "Optional ordered recovery candidates after the root attempt. Each uses the original task prompt and every reference image, with its own complete parameters; parameters are replaced, never merged from the previous route. Choose suitable models from the current catalog and put each model_ref in that candidate's parameters.model. Do not supply prompt/source fields, n or batchSize in candidate parameters. count repeats the entire bounded chain. The host retries each candidate on confirmed output download/persistence 502 failures up to 3 times with 2/4/8-second backoff before advancing. Success/partial/pending/canceled or uncertain outcomes stop; only confirmed retryable failures advance. Each retry resubmits generation and may incur charges. No automatic model selection or caller-declared duplicate candidates.",
        "items": {
            "type": "object", "required": ["type", "parameters"],
            "properties": {"type": kind, "parameters": {"$ref": "#/properties/parameters"}, "wait": {"$ref": "#/properties/wait"}},
            "additionalProperties": false
        }
    })
}

fn prompt_requirements() -> Value {
    let prompt = json!({"type": "string", "minLength": 1, "pattern": "\\S"});
    json!([
        {"required": ["prompt"], "properties": {"prompt": prompt}},
        {"required": ["type"], "properties": {"type": {"enum": [
            "free-imitation", "outpaint", "super-resolution", "split-layers", "separate-layers",
            "enhance", "repeat-horizontal", "convert", "line-extraction", "color-transfer",
            "blend", "erase", "watermark-erase", "extract", "lineart", "vectorize",
            "three-views", "bleed-line", "upscale", "super-upscale", "extract-color", "save-color",
            "detect-grid", "classify-intent", "build-extract-prompts", "background-remove",
            "f-tools", "check-image", "micro-upscale", "micro-upscale-image", "product-kit",
            "a-plus", "batch-shape-fill", "batch-watermark", "batch-enhance", "batch-upscale",
            "batch-background-remove", "batch-extract-pattern", "batch-replace-scene", "batch-mockup"
        ]}}},
        {
            "allOf": [
                {"required": ["parameters"], "properties": {
                    "parameters": {"required": ["prompt"], "properties": {"prompt": prompt}}
                }},
                {"anyOf": [
                {"required": ["type"], "properties": {"type": {"enum": [
                    "variation", "extend", "mix", "pattern-apply", "material-product", "ip-apply",
                    "extract-pattern", "video", "model-scene", "background", "modify", "seed-edit",
                    "video-auto-director", "video-remake-director", "micro-generate", "micro-variation",
                    "scheme-generate", "faddish", "g-tools", "a-plus-edit", "batch-generate", "batch-series-extend"
                ]}}},
                {"required": ["images"], "properties": {"images": {"type": "array", "items": {}, "minItems": 1}, "type": {"enum": ["auto"]}}}
                ]}
            ]
        }
    ])
}

fn image_type_schema() -> Value {
    json!({
        "type": "string",
        "enum": [
            "auto", "generate", "edit", "variation", "extend", "mix",
            "fission", "pattern-apply", "free-imitation", "material-product",
            "ip-apply", "outpaint", "super-resolution", "split-layers",
            "separate-layers", "enhance", "extract-pattern", "repeat-horizontal",
            "convert", "line-extraction", "color-transfer",
            "video", "model-scene", "background", "modify", "seed-edit", "blend",
            "erase", "watermark-erase", "extract", "lineart", "vectorize", "three-views",
            "bleed-line", "upscale", "super-upscale", "video-auto-director", "video-remake-director",
            "extract-color", "save-color", "detect-grid", "classify-intent", "build-extract-prompts",
            "background-remove", "micro-generate", "micro-variation", "scheme-generate", "faddish",
            "g-tools", "f-tools", "check-image", "micro-upscale", "micro-upscale-image",
            "product-kit", "a-plus", "a-plus-edit", "batch-shape-fill", "batch-generate",
            "batch-watermark", "batch-series-extend", "batch-enhance", "batch-upscale",
            "batch-background-remove", "batch-extract-pattern", "batch-replace-scene", "batch-mockup"
        ],
        "default": "auto",
        "description": "Prefer an explicit type whose documented functions, inputs, outputs and limits cover the task; the defaults below do not establish support for every image request: micro-generate for ordinary platform text-to-image when suitable; generate for complex/free-form text-to-image through Fusion images/generations; variation for modifying one source image; mix for fusing 2-10 references in input order; extend for four-panel grids or same-series extension from one base image. With source images and a selected catalog model, use explicit edit through Fusion images/edits; preserve the references. A catalog model argument on variation/extend/mix does not select that Fusion model. generate/edit need the selected opaque model_ref from list_iyw_image_models. Prioritize applicable platform features; choose image models for complex tasks, freer creative control, explicit requests, or platform limitations/unavailability/confirmed failure. Platform model one/two/three select modelChannel=1/2/3, not Fusion catalog positions. Built-in image-to-3d is disabled. fission and matching specialized platform types remain explicitly available. auto uses generate with no images, extend with one image and series/extension/four-panel/2x2 wording, variation for other single-image prompts, and mix for multiple images. A grid request without a reference uses generate; do not drop extra references to force extend."
    })
}

fn image_sources_schema() -> Value {
    json!({
        "type": "array",
        "minItems": 0,
        "maxItems": MAX_IMAGE_INPUTS,
        "description": "Use the exact image attachment paths supplied in this turn, existing workspace paths, file URIs, HTTPS URLs or Data URLs. Current-session managed attachments may be outside the workspace; other outside files and other sessions' attachments are rejected. Do not guess paths or search directories for supplied attachments. edit accepts up to 16 PNG/JPEG/WebP references plus one PNG mask with role=mask. All other operations retain their documented limits (at most 10). Local and inspected inputs are limited to 20 MiB. HTTPS references are submitted directly; their content limits are enforced upstream. Masks and the first source are inspected for matching dimensions.",
        "items": {
            "oneOf": [
                {"type": "string", "minLength": 1},
                {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string", "minLength": 1},
                        "path": {"type": "string", "minLength": 1, "description": "Exact supplied attachment path, existing workspace file path or file URI. Relative paths resolve from the session workspace. Preserve Chinese characters and spaces; do not URL-encode filesystem paths."},
                        "base64": {"type": "string", "minLength": 1},
                        "data": {"type": "string", "minLength": 1},
                        "mimeType": {"type": "string", "minLength": 1},
                        "role": {"type": "string", "maxLength": 64},
                        "name": {"type": "string", "maxLength": 255}
                    },
                    "additionalProperties": false
                }
            ]
        }
    })
}

fn wait_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "timeoutSeconds": {
                "type": "integer", "minimum": 0,
                "description": "Agent-controlled timeout override in seconds; values above 600 are supported. Omit for 600 seconds on standard platform requests/polling, 650 seconds on product-kit/a-plus HTTP, 120 seconds on batch-center HTTP (polling wait 600), or 300 seconds on generate/edit. Positive values override the operation's HTTP timeout and platform polling wait, including per-item waits in a batch. Prefer the defaults or longer for slow image tasks; avoid premature termination. 0 submits platform tasks without polling; generate/edit still use their default HTTP timeout."
            },
            "pollIntervalSeconds": {"type": "number", "exclusiveMinimum": 0, "maximum": 30, "default": 2}
        },
        "additionalProperties": false
    })
}

fn delivery_schema() -> Value {
    json!({
        "type": "object",
        "description": "Artifact registration defaults to off. Keep generated outputs unregistered while creating, comparing or selecting candidates. Set registerArtifact=true only when the image itself is selected as a final file to give the user in this conversation. For images used inside a PPT, report, webpage or other composed deliverable, leave it false and register the completed deliverable with present_task_files after the task is complete. For batches, this top-level setting applies to every item. The agent selects final deliverables from the task without separate user approval.",
        "properties": {
            "display": {
                "type": "boolean",
                "default": false,
                "description": "Compatibility option; does not independently register images. Artifact registration is controlled by registerArtifact."
            },
            "registerArtifact": {"type": "boolean", "default": false, "description": "Defaults to false so candidate images and production materials are not registered. Set true only when the image itself has been selected as a final file to give the user in this conversation. Leave false for images embedded in another deliverable; register the completed file later with present_task_files."}
        },
        "additionalProperties": false
    })
}
