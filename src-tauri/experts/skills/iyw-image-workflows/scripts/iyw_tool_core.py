"""固定图片工具别名及请求体校验。"""

from __future__ import annotations

from typing import Any, Callable

from iyw_commerce_core import _require_https, _validate_payload_safety
from iyw_image import IywError


TOOL_OPERATIONS = {
    "variation": "g_tools_generate_image",
    "extend": "g_tools_generate_image",
    "mix": "g_tools_generate_image",
    "pattern-apply": "g_tools_generate_image",
    "free-imitation": "fission",
    "material-product": "g_tools_generate_image",
    "ip-apply": "g_tools_generate_image",
    "edit": "erase",
    "outpaint": "outpainting",
    "super-resolution": "SuperResolution",
    "split-layers": "f_tools",
    "separate-layers": "g_tools_generate_image",
    "enhance": "EnhanceImage",
    "extract-pattern": "g_tools_generate_image",
    "repeat-horizontal": "g_tools_generate_image",
    "convert": "convert",
    "line-extraction": "lineExtraction",
    "color-transfer": "g_tools_generate_image",
    "image-to-3d": "ImageTo3D",
    "video": "videoGenerator",
    "model-scene": "modelScene",
}

IMAGE_FORMATS = frozenset(
    {"psd", "svg", "bmp", "gif", "ico", "jpeg", "jpg", "odd", "png", "tiff", "webp"}
)
RATIOS = frozenset({"auto", "1:1", "4:3", "3:4", "16:9", "9:16", "21:9"})


def _string(payload: dict[str, Any], key: str, *, required: bool = True) -> str:
    value = payload.get(key)
    if not isinstance(value, str) or (required and not value.strip()):
        raise IywError(f"{key} must be a non-empty string", "invalid_input")
    return value


def _number(payload: dict[str, Any], key: str, *, minimum: float = 0, maximum: float | None = None) -> None:
    value = payload.get(key)
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise IywError(f"{key} must be a number", "invalid_input")
    if value < minimum or (maximum is not None and value > maximum):
        raise IywError(f"{key} is outside the supported range", "invalid_input")


def _image(payload: dict[str, Any], key: str = "image") -> str:
    return _require_https(_string(payload, key), f"{key} URL")


def _image_list(payload: dict[str, Any], key: str = "imageUrls", *, minimum: int = 1, maximum: int = 10) -> list[str]:
    values = payload.get(key)
    if isinstance(values, str):
        values = [values]
    if not isinstance(values, list) or not minimum <= len(values) <= maximum:
        raise IywError(f"{key} must contain {minimum} to {maximum} image URLs", "invalid_input")
    return [_require_https(_string({"url": value}, "url"), f"{key} URL") for value in values]


def _g_tools(payload: dict[str, Any], tool_name: str, *, images: int | None = None) -> None:
    payload["toolName"] = tool_name
    values = _image_list(payload, minimum=images or 1, maximum=images or 10)
    if images == 1:
        payload["imageUrls"] = values[0]
    elif isinstance(payload.get("imageUrls"), str):
        payload["imageUrls"] = values


def _validate_variation(payload: dict[str, Any]) -> None:
    _g_tools(payload, "variation", images=1)
    _string(payload, "prompt")


def _validate_extend(payload: dict[str, Any]) -> None:
    _g_tools(payload, "extend", images=1)
    _string(payload, "prompt")


def _validate_mix(payload: dict[str, Any]) -> None:
    _g_tools(payload, "mix")
    if not 2 <= len(payload["imageUrls"]) <= 10:
        raise IywError("mix requires 2 to 10 image URLs", "invalid_input")
    _string(payload, "prompt")


def _validate_pattern_apply(payload: dict[str, Any]) -> None:
    _g_tools(payload, "iyw_tu", images=2)
    if not isinstance(payload.get("product"), list) or not isinstance(payload.get("material"), list):
        raise IywError("pattern-apply requires product and material metadata", "invalid_input")
    _string(payload, "prompt")


def _validate_free_imitation(payload: dict[str, Any]) -> None:
    _image(payload, "reference")
    stats = payload.get("stats")
    if not isinstance(stats, dict):
        raise IywError("free-imitation requires stats", "invalid_input")
    for key in ("width", "height", "strength"):
        _number(stats, key, minimum=0)
    if payload.get("model") != "free":
        raise IywError('free-imitation model must be "free"', "invalid_input")


def _validate_material_product(payload: dict[str, Any]) -> None:
    _g_tools(payload, "user_product", images=2)
    if not isinstance(payload.get("product"), dict) or not isinstance(payload.get("material"), list):
        raise IywError("material-product requires product and material metadata", "invalid_input")
    _string(payload, "prompt")


def _validate_ip_apply(payload: dict[str, Any]) -> None:
    _g_tools(payload, "iyw_ip")
    if not isinstance(payload.get("product"), dict) or not isinstance(payload.get("jsonData"), dict):
        raise IywError("ip-apply requires product and jsonData metadata", "invalid_input")
    _string(payload, "prompt")


def _validate_edit(payload: dict[str, Any]) -> None:
    _image(payload)
    _image(payload, "mask")
    _string(payload, "prompt")


def _validate_outpaint(payload: dict[str, Any]) -> None:
    _image(payload)
    for key in ("top", "right", "bottom", "left"):
        _number(payload, key, minimum=0, maximum=1)


def _validate_super_resolution(payload: dict[str, Any]) -> None:
    _image(payload, "reference")
    if payload.get("upscale") not in {2, 4}:
        raise IywError("upscale must be 2 or 4", "invalid_input")


def _validate_split_layers(payload: dict[str, Any]) -> None:
    _image(payload, "reference")
    if payload.get("model") != "extract_layers":
        raise IywError('split-layers model must be "extract_layers"', "invalid_input")


def _validate_separate_layers(payload: dict[str, Any]) -> None:
    _g_tools(payload, "seperate_layers", images=1)


def _validate_enhance(payload: dict[str, Any]) -> None:
    _image(payload)
    if not isinstance(payload.get("enhanceType"), int) or payload["enhanceType"] not in {1, 2}:
        raise IywError("enhanceType must be 1 or 2", "invalid_input")
    if not isinstance(payload.get("model"), int):
        raise IywError("model must be an integer", "invalid_input")


def _validate_extract_pattern(payload: dict[str, Any]) -> None:
    _g_tools(payload, "extract_pattern", images=1)
    _string(payload, "prompt")


def _validate_repeat_horizontal(payload: dict[str, Any]) -> None:
    payload["toolName"] = "return_leftright"
    payload["imageUrls"] = _image_list(payload, minimum=1, maximum=1)


def _validate_convert(payload: dict[str, Any]) -> None:
    _image(payload)
    for key in ("inputFormat", "outputFormat"):
        value = _string(payload, key).lower()
        if value not in IMAGE_FORMATS:
            raise IywError(f"unsupported image format: {value}", "invalid_input")


def _validate_line_extraction(payload: dict[str, Any]) -> None:
    _image(payload, "reference")
    stats = payload.get("stats")
    if not isinstance(stats, dict):
        raise IywError("line-extraction requires stats", "invalid_input")
    _require_https(_string(stats, "reference"), "stats reference URL")
    if payload.get("model") not in {"realistic", "canny"}:
        raise IywError("model must be realistic or canny", "invalid_input")
    if not isinstance(payload.get("batch_size"), int) or payload["batch_size"] < 1:
        raise IywError("batch_size must be positive", "invalid_input")


def _validate_color_transfer(payload: dict[str, Any]) -> None:
    values = _image_list(payload, minimum=2, maximum=2)
    payload["imageUrls"] = values
    _image(payload, "productImg")
    _image(payload, "styleImg")
    if payload.get("resolution") not in {"2K", "4K"}:
        raise IywError("resolution must be 2K or 4K", "invalid_input")
    payload["toolName"] = "color_transfer"


def _validate_image_to_3d(payload: dict[str, Any]) -> None:
    _image(payload)
    stats = payload.get("stats")
    if not isinstance(stats, dict) or not isinstance(stats.get("format"), int):
        raise IywError("image-to-3d requires stats.format", "invalid_input")
    views = stats.get("MultiViewImages")
    if views is not None:
        if not isinstance(views, list):
            raise IywError("MultiViewImages must be a list", "invalid_input")
        for view in views:
            if not isinstance(view, dict):
                raise IywError("each 3D view must be an object", "invalid_input")
            _require_https(_string(view, "ViewImageUrl"), "3D view URL")


def _validate_video(payload: dict[str, Any]) -> None:
    _image(payload, "reference")
    _string(payload, "prompt")
    if payload.get("ratio") not in RATIOS - {"auto"}:
        raise IywError("video ratio is invalid", "invalid_input")
    if not isinstance(payload.get("duration"), int) or not 4 <= payload["duration"] <= 15:
        raise IywError("video duration must be 4 to 15 seconds", "invalid_input")
    if payload.get("mode") not in {"normal", "hd"}:
        raise IywError("video mode must be normal or hd", "invalid_input")


def _validate_model_scene(payload: dict[str, Any]) -> None:
    _image_list(payload)
    _string(payload, "prompt")
    if payload.get("size") not in RATIOS:
        raise IywError("model-scene size is invalid", "invalid_input")
    if payload.get("resolution") not in {"standard", "4K"}:
        raise IywError("model-scene resolution is invalid", "invalid_input")


VALIDATORS: dict[str, Callable[[dict[str, Any]], None]] = {
    "variation": _validate_variation,
    "extend": _validate_extend,
    "mix": _validate_mix,
    "pattern-apply": _validate_pattern_apply,
    "free-imitation": _validate_free_imitation,
    "material-product": _validate_material_product,
    "ip-apply": _validate_ip_apply,
    "edit": _validate_edit,
    "outpaint": _validate_outpaint,
    "super-resolution": _validate_super_resolution,
    "split-layers": _validate_split_layers,
    "separate-layers": _validate_separate_layers,
    "enhance": _validate_enhance,
    "extract-pattern": _validate_extract_pattern,
    "repeat-horizontal": _validate_repeat_horizontal,
    "convert": _validate_convert,
    "line-extraction": _validate_line_extraction,
    "color-transfer": _validate_color_transfer,
    "image-to-3d": _validate_image_to_3d,
    "video": _validate_video,
    "model-scene": _validate_model_scene,
}


def validate_tool_payload(alias: str, payload: dict[str, Any]) -> str:
    if alias not in TOOL_OPERATIONS:
        raise IywError(f"unsupported tool: {alias}", "invalid_input")
    if not isinstance(payload, dict):
        raise IywError("tool payload must be a JSON object", "invalid_input")
    _validate_payload_safety(payload)
    VALIDATORS[alias](payload)
    return TOOL_OPERATIONS[alias]
