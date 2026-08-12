"""固定图片工具别名到 Commerce operation 的映射。"""

DESIGN_MODEL_CHANNEL = 2

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
