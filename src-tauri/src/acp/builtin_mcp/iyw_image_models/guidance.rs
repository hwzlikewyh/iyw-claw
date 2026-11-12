pub(super) fn with_selection_guidance(
    model_id: Option<&str>,
    description: Option<&str>,
) -> Option<String> {
    let guidance = match model_id {
        Some("gpt-image-2-low") => {
            "选型建议：适合速度优先的初稿探索、并行风格比较、高频修改和批量候选。为方便比较，请保持主体、必要文案与关键约束一致，每张只改变一到两个视觉方向；用户选定构图后，再用高精度版本完成终稿。具体 quality、size 等请求参数必须以本模型 image_capabilities 明确列出的取值和限制为准，不能只根据模型 ID 推断。"
        }
        Some("gpt-image-2-high") => {
            "选型建议：适合用户明确要求高精度、细节丰富或最终交付的场景，包括文字密集的版式、商品包装、信息图、界面概念图和商业成品。需要快速比较多个构思时，先并行生成低精度候选，再将选定方案交给高精度版本完成；只有在用户要求高精度候选对比时，才让每个候选都使用本版本。具体 quality、size 等请求参数必须以本模型 image_capabilities 明确列出的取值和限制为准，不能只根据模型 ID 推断。"
        }
        _ => return description.map(str::to_owned),
    };
    let mut result = description.unwrap_or_default().to_owned();
    if !result.is_empty() {
        result.push_str("\n\n");
    }
    result.push_str(guidance);
    result.push_str(
        "\n\n当前 Fusion 中转只接受完整 JSON 返回；上游模型描述中的流式能力不代表该调用路径支持图片 SSE，请勿仅据此启用 stream 或 partial_images。",
    );
    Some(result)
}
