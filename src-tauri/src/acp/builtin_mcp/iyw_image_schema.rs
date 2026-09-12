use serde_json::{json, Value};

use super::tool_identity::IMAGE_TOOL;

pub(super) fn tool() -> Value {
    json!({
        "name": IMAGE_TOOL,
        "description": "Generate or edit images using the route that matches the task. Text-to-image has priority on Fusion type=generate (images/generations), including auto without source images. Fusion type=edit (images/edits) is available when explicitly selected and requires at least one source image. Neither requires a prior platform attempt or failure. For either Fusion operation, first call list_iyw_image_models, select a model supporting the operation, and pass its exact id in parameters.model; the host never chooses a default model. Reuse the catalog for the same task or batch. For one-image redesign or color/material/detail changes, prefer platform variation. For combining 2-10 references, prefer mix. For four-panel grids or same-series extension from one reference, prefer extend; keep one base image and describe the grid/series in prompt. With no base image, generate the requested composition using generate. Platform operations need no Fusion model lookup; fission remains available when explicitly selected. Use a matching specialized operation for outpaint, background, super-resolution and similar tasks. Default timeouts: standard platform requests/polling 600 seconds, product-kit/a-plus HTTP 650 seconds, batch-center submit HTTP 120 seconds (polling wait 600), Fusion 300 seconds; wait.timeoutSeconds can override either, including above 600. Prefer the defaults or longer for slow tasks. This tool has no image-generation capability_id; use its advertised definition without capability search/read/invoke, separate upload, or another image Skill. Use single-task fields for one task, or requests for up to eight independent tasks. count starts multiple charged executions; never combine it with parameters.n or parameters.batchSize. A batch is validated before execution, continues after runtime item failures, and returns partial results in input order with successful URLs registered together. Deliver ordinary successful images using returned status, URLs, and delivery metadata. Inspect visuals for requested review, comparison, visual acceptance, or integration into a composed deliverable; a detailed prompt alone does not require review. Report partial or failed status honestly and do not regenerate beyond scope. Explain the selected operation/backend when asked about routing; the tool name or successful output alone does not prove platform execution. A backend rejection is not a tool-identity error; preserve it without inventing capability IDs. A timeout, transport error, or non-terminal result is not confirmed generation failure: query the original task_id when available; never blindly retry or switch to edit/generate after uncertain submission.",
        "inputSchema": tool_input_schema()
    })
}

fn parameters_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": true,
        "description": "Only fields supported by the selected operation. For generate, auto without images, or explicit edit, call list_iyw_image_models and set model to the exact selected id: generate requires capabilities.image_generation=true, edit requires capabilities.image_editing=true. Choose from returned descriptions, capabilities, prices and user requirements. No prior platform failure is required. Missing model IDs and display names are rejected; no default model is selected. Other Fusion options must be supported by the model. variation, extend and mix need only prompt and images for a default result; their toolName and modelChannel are host-owned. Do not copy parameters across operations. Existing specialized parameters: free-imitation requires model=free and stats.width/height/strength; outpaint top/right/bottom/left in [0,1]; super-resolution upscale=2|4; split-layers model=extract_layers; enhance enhanceType=1|2 and integer model; convert inputFormat/outputFormat in png/jpg/jpeg/webp/gif/bmp; line-extraction model=realistic|canny, positive batch_size, stats.reference; image-to-3d stats.format and optional stats.MultiViewImages[].ViewImageUrl; video ratio, duration=4..15, mode=normal|hd; model-scene/background size and resolution=standard|4K. New native types: modify/seed-edit/micro-variation need images+prompt; blend 2-10 images, prompt optional; erase needs HTTPS mask and accepts prompt/payOrderNo; watermark-erase accepts target=text|watermark|text_watermark or a HTTPS mask; lineart accepts style; upscale/super-upscale accept upscale/op; vectorize/three-views/extract/detect-grid/build-extract-prompts need images; bleed-line needs size or bleed from the actual page; extract-color needs images; save-color needs colors; classify-intent needs text or keys; background-remove/check-image need one image; micro-upscale/micro-upscale-image need one image and known amount/device/payMethod/typeId; video-auto-director/video-remake-director need images+prompt and documented ratio/duration/mode; micro-generate needs prompt and optional images/modelChannel/ratio/batchSize/tool; scheme-generate needs an existing schemeId+prompt; faddish needs prompt and optional images; g-tools needs prompt and optional images/model; f-tools needs content and documented price/type, or toolName plus imageUrls from the current page. product-kit/a-plus need images and platform/market/language/contentType/resolution/productInfo/modules; a-plus also needs selectedPlan exactly from fetch_iyw_url product-kit/plan-a-plus. a-plus-edit needs one image+prompt and optional size/stats/jsonData; the host fixes variation/toolType=12/modelChannel=2/batchSize=1/isChange=1. Batch-center types batch-shape-fill, batch-generate, batch-watermark, batch-series-extend, batch-enhance, batch-upscale, batch-background-remove, batch-extract-pattern, batch-replace-scene, batch-mockup accept images and per-input batchSize (default 1). batch-generate/series-extend need prompt; watermark needs target=text|watermark|text_watermark; upscale scale=2..8 defaults 2; enhance defaults enhanceType=2/model=0; mockup requires mockupId. Remaining fields come from the selected batch tool. Use the gateway Skill's product-kit, batch-center and enum references for detailed contracts. Batch-center metadata.batch_id and metadata.query identify the status request; keep batchId distinct from taskId. Planning, copy, history, status, image-version saving and downloads all use fetch_iyw_url. Unknown field types/enums must come from an actual page contract. Data, colors, prompts and vector/video/3D links are in metadata.result, including batch runs. Query existing tasks with fetch_iyw_url and metadata.query; do not repeat generation after polling errors.",
        "properties": {
            "mask": {"type": "string", "description": "erase requires a public HTTPS mask URL; watermark-erase accepts mask or target; upload a local mask with upload_iyw_file first."},
            "strength": {"type": "number", "minimum": 0},
            "text": {"type": "string", "minLength": 1, "description": "classify-intent: current canvas instruction text."},
            "target": {"type": "string", "enum": ["text", "watermark", "text_watermark"], "description": "watermark-erase or batch-watermark target."},
            "selectedPlan": {"type": "object", "description": "a-plus: exact plan object returned by product-kit/plan-a-plus."},
            "modules": {"type": "array", "minItems": 1, "description": "product-kit/a-plus: selected module keys from the current product-kit workflow."},
            "upscale": {"type": "number", "minimum": 0, "description": "Operation-specific scale. super-resolution only accepts 2 or 4; other operations follow their own contract."},
            "colors": {"type": "array", "minItems": 1, "description": "save-color: use entries from extract-color output."},
            "keys": {"type": "array", "minItems": 1, "description": "classify-intent: known canvas context keys; the current page can instead send parameters.text."},
            "schemeId": {"type": ["string", "integer"], "description": "scheme-generate: existing design scheme ID from a business query."},
            "stats": {"type": "object", "description": "Operation-specific structure: image-to-3d format/MultiViewImages or line-extraction reference."},
            "models": {"type": "array", "items": {"type": "object"}, "description": "fission: confirmed model configuration objects, not display-name strings."}
        }
    })
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
        "type": "array", "minItems": 1, "maxItems": 8,
        "description": "Batch only. Each item is a complete image request. Do not mix requests with root type, prompt, images, parameters, count, or wait.",
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
            "maximum": 4,
            "default": 1,
            "description": "Intentional execution count. Do not combine with parameters.n or parameters.batchSize."
        },
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

fn prompt_requirements() -> Value {
    let prompt = json!({"type": "string", "minLength": 1, "pattern": "\\S"});
    json!([
        {"required": ["prompt"], "properties": {"prompt": prompt}},
        {"required": ["type"], "properties": {"type": {"enum": [
            "free-imitation", "outpaint", "super-resolution", "split-layers", "separate-layers",
            "enhance", "repeat-horizontal", "convert", "line-extraction", "color-transfer",
            "image-to-3d", "blend", "erase", "watermark-erase", "extract", "lineart", "vectorize",
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
            "convert", "line-extraction", "color-transfer", "image-to-3d",
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
        "description": "Prefer an explicit type matching the task: generate for text-to-image through Fusion images/generations; variation for modifying one source image; mix for fusing 2-10 references in input order; extend for four-panel grids or same-series extension from one base image. Explicit edit uses Fusion images/edits and requires source images. generate/edit need a selected model ID from list_iyw_image_models, with no prior platform attempt or failure. fission and matching specialized platform types remain explicitly available. auto uses generate with no images, extend with one image and series/extension/four-panel/2x2 wording, variation for other single-image prompts, and mix for multiple images. A grid request without a reference uses generate; do not drop extra references to force extend."
    })
}

fn image_sources_schema() -> Value {
    json!({
        "type": "array",
        "minItems": 0,
        "maxItems": 10,
        "items": {
            "oneOf": [
                {"type": "string", "minLength": 1},
                {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string", "minLength": 1},
                        "path": {"type": "string", "minLength": 1},
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
        "description": "Register only images that are themselves final deliverables. For intermediate images used inside a PPT, report or webpage, set registerArtifact=false and register only the completed deliverable later. Select final deliverables from the task without requiring separate user approval.",
        "properties": {
            "display": {
                "type": "boolean",
                "default": false,
                "description": "Compatibility option. Image results are delivered to Artifacts; the delivery host downloads and deduplicates image URLs automatically. No separate agent download is needed."
            },
            "registerArtifact": {"type": "boolean", "default": true, "description": "Set false for intermediate assets, drafts and embedded document illustrations, including images generated for a PPT. Leave true when the images themselves are final deliverables."}
        },
        "additionalProperties": false
    })
}
