use serde_json::{json, Value};

use super::tool_identity::IMAGE_TOOL;

pub(super) fn tool() -> Value {
    json!({
        "name": IMAGE_TOOL,
        "description": "Create images with IYW platform operations first: fission for text-only creation, variation for one-image redesign or color/material/detail changes, extend for series extension, mix for multiple references, or the matching specialized platform operation. This direct tool wraps both platform and Fusion backends: type=generate calls Fusion images/generations and type=edit calls Fusion images/edits. Direct-tool priority does not authorize these fallback types. Both fallback conditions must hold: an IYW platform attempt for the same task explicitly failed or was rejected before task creation, and list_iyw_image_models returned a suitable model whose exact id you pass in parameters.model. Missing IDs and display names are rejected; the host never chooses a default model. Local path/parameter errors and complex prompts do not permit fallback. Reuse the catalog for the same task or batch. Platform operations and auto need no Fusion lookup. Default timeouts: standard platform requests/polling 600 seconds, product-kit/a-plus HTTP 650 seconds, batch-center submit HTTP 120 seconds (polling wait 600), Fusion 300 seconds; wait.timeoutSeconds can override either, including above 600. Prefer the defaults or longer for slow tasks. This tool has no image-generation capability_id; use its advertised definition without capability search/read/invoke, separate upload, or another image Skill. Use single-task fields for one task, or requests for up to eight independent tasks. count starts multiple charged executions; never combine it with parameters.n or parameters.batchSize. A batch is validated before execution, continues after runtime item failures, and returns partial results in input order with successful URLs registered together. Deliver ordinary successful images using returned status, URLs, and delivery metadata. Inspect visuals for requested review, comparison, visual acceptance, or integration into a composed deliverable; a detailed prompt alone does not require review. Report partial or failed status honestly and do not regenerate beyond scope. Explain the selected operation/backend when asked about routing; the tool name or successful output alone does not prove platform execution. A backend rejection is not a tool-identity error; preserve it without inventing capability IDs. A timeout, transport error, or non-terminal result is not confirmed generation failure: query the original task_id when available; never blindly retry or switch to edit/generate after uncertain submission.",
        "inputSchema": {
            "type": "object",
            "oneOf": [single_image_schema(), batch_image_schema()]
        }
    })
}

fn parameters_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": true,
        "description": "Only fields supported by the selected operation. variation, extend, and mix need only prompt and images for a default result. The host sets their toolName and modelChannel; do not guess them or copy parameters across operations. generate/edit are Fusion fallbacks only after a confirmed platform failure for the same task. Then call list_iyw_image_models and set model to the exact id you select: generate requires capabilities.image_generation=true, edit requires capabilities.image_editing=true. Choose from returned descriptions, capabilities, prices, and user requirements. Missing model IDs and display names are rejected; no default model is selected. Other Fusion options must be supported by the selected model. Existing specialized parameters: free-imitation requires model=free and stats.width/height/strength; outpaint top/right/bottom/left in [0,1]; super-resolution upscale=2|4; split-layers model=extract_layers; enhance enhanceType=1|2 and integer model; convert inputFormat/outputFormat in png/jpg/jpeg/webp/gif/bmp; line-extraction model=realistic|canny, positive batch_size, stats.reference; image-to-3d stats.format and optional stats.MultiViewImages[].ViewImageUrl; video ratio, duration=4..15, mode=normal|hd; model-scene/background size and resolution=standard|4K. New native types: modify/seed-edit/micro-variation need images+prompt; blend 2-10 images, prompt optional; erase needs HTTPS mask and accepts prompt/payOrderNo; watermark-erase accepts target=text|watermark|text_watermark or a HTTPS mask; lineart accepts style; upscale/super-upscale accept upscale/op; vectorize/three-views/extract/detect-grid/build-extract-prompts need images; bleed-line needs size or bleed from the actual page; extract-color needs images; save-color needs colors; classify-intent needs text or keys; background-remove/check-image need one image; micro-upscale/micro-upscale-image need one image and known amount/device/payMethod/typeId; video-auto-director/video-remake-director need images+prompt and documented ratio/duration/mode; micro-generate needs prompt and optional images/modelChannel/ratio/batchSize/tool; scheme-generate needs an existing schemeId+prompt; faddish needs prompt and optional images; g-tools needs prompt and optional images/model; f-tools needs content and documented price/type, or toolName plus imageUrls from the current page. product-kit/a-plus need images and platform/market/language/contentType/resolution/productInfo/modules; a-plus also needs selectedPlan exactly from fetch_iyw_url product-kit/plan-a-plus. a-plus-edit needs one image+prompt and optional size/stats/jsonData; the host fixes variation/toolType=12/modelChannel=2/batchSize=1/isChange=1. Batch-center types batch-shape-fill, batch-generate, batch-watermark, batch-series-extend, batch-enhance, batch-upscale, batch-background-remove, batch-extract-pattern, batch-replace-scene, batch-mockup accept images and per-input batchSize (default 1). batch-generate/series-extend need prompt; watermark needs target=text|watermark|text_watermark; upscale scale=2..8 defaults 2; enhance defaults enhanceType=2/model=0; mockup requires mockupId. Remaining fields come from the selected batch tool. Use the gateway Skill's product-kit, batch-center and enum references for detailed contracts. Batch-center metadata.batch_id and metadata.query identify the status request; keep batchId distinct from taskId. Planning, copy, history, status, image-version saving and downloads all use fetch_iyw_url. Unknown field types/enums must come from an actual page contract. Data, colors, prompts and vector/video/3D links are in metadata.result, including batch runs. Query existing tasks with fetch_iyw_url and metadata.query; do not repeat generation after polling errors.",
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

fn single_image_schema() -> Value {
    let mut schema = image_request_schema(false);
    schema["properties"]["delivery"] = delivery_schema();
    schema
}

fn batch_image_schema() -> Value {
    json!({
        "type": "object",
        "required": ["requests"],
        "properties": {
            "requests": {
                "type": "array",
                "minItems": 1,
                "maxItems": 8,
                "items": image_request_schema(true)
            },
            "delivery": delivery_schema()
        },
        "additionalProperties": false
    })
}

fn image_request_schema(include_id: bool) -> Value {
    let mut properties = json!({
        "type": image_type_schema(),
        "prompt": {"type": "string", "maxLength": 12000, "description": "State what to preserve and change, the intended layout, and each reference image's role in input order. Select the operation with type; the prompt alone does not select specialized tools."},
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
        properties["id"] = json!({"type": "string", "minLength": 1, "maxLength": 64});
    }
    json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false
    })
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
        "description": "Prefer an explicit IYW platform type. Use fission for text-only creation. With one source image, use variation for redesign or color/material/detail changes, and extend for same-series or trend/theme extension. With 2-10 references, use mix and preserve input order. Use the matching specialized platform type for background, outpaint, super-resolution, pattern, layer, color, format, 3D, or video tasks. generate (Fusion images/generations) and edit (Fusion images/edits) are fallback-only types after a confirmed platform failure for the same task plus explicit model selection from list_iyw_image_models. The word editing in the user's request does not imply type=edit. auto routes no images to fission, one image to variation or extend for series/extension wording, and multiple images to mix; it does not infer specialized operations."
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
        "properties": {
            "display": {
                "type": "boolean",
                "default": false,
                "description": "Compatibility option. Image results are delivered to Artifacts; the delivery host downloads and deduplicates image URLs automatically. No separate agent download is needed."
            },
            "registerArtifact": {"type": "boolean", "default": true}
        },
        "additionalProperties": false
    })
}
