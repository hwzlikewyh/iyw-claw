from pathlib import Path


SKILL = Path(__file__).parents[1] / "iyw-image-workflows" / "SKILL.md"
AGENT_CONFIG = Path(__file__).parents[1] / "iyw-image-workflows" / "agents" / "openai.yaml"
COMMERCE_REFERENCE = (
    Path(__file__).parents[1]
    / "iyw-image-workflows"
    / "references"
    / "commerce-operations.md"
)


def test_skill_documents_image_priority_and_fixed_tools():
    text = SKILL.read_text(encoding="utf-8")
    frontmatter = text.split("---", 2)[1]
    assert "图片请求按图片输入优先级路由" in frontmatter
    assert "有基准图片且指定趋势或主题时优先系列延伸" in frontmatter
    assert "使用已封装的 `tool variation` 命令变款" in text
    assert "CLI 会调用 `g_tools_generate_image`" in text
    assert "payload 的 `toolName` 固定为" in text
    assert "不要把它当作接口 operation" in text
    assert "scripts/iyw_search.py" in text
    assert "该别名内部调用 `g_tools_generate_image`" in text


def test_agent_prompt_uses_g_tools_generate_image_for_variation():
    text = AGENT_CONFIG.read_text(encoding="utf-8")
    assert "使用已封装的 tool variation" in text
    assert "调用 g_tools_generate_image 并固定 toolName" in text


def test_fission_generation_defaults_to_one_platform_and_compares_explicitly():
    skill = SKILL.read_text(encoding="utf-8")
    agent = AGENT_CONFIG.read_text(encoding="utf-8")

    assert "默认只向一个平台下发" in skill
    assert "优先使用通道四" in skill
    assert "回退到实时配置顺序中的第一个可用平台" in skill
    assert "`--compare-platforms`" in skill
    assert "明确要求多平台比稿" in skill
    assert "默认单平台并优先通道四" in agent
    assert "明确要求多平台比稿时才传 --compare-platforms" in agent


def test_trend_routing_prefers_extend_with_bounded_variation_fallback():
    skill = SKILL.read_text(encoding="utf-8")
    agent = AGENT_CONFIG.read_text(encoding="utf-8")
    reference = COMMERCE_REFERENCE.read_text(encoding="utf-8")

    assert "基准图片并指定趋势或主题" in skill
    assert "优先执行 `tool extend`" in skill
    assert "没有基准图片" in skill and "`fission-generate`" in skill
    assert "只回退一次 `tool variation`" in skill
    assert "创建结果不确定" in reference
    assert "只查询原 task ID" in reference
    assert "趋势或主题且有基准图片时优先 tool extend" in agent


def test_grouped_layout_uses_one_task_before_local_composition():
    skill = SKILL.read_text(encoding="utf-8")
    agent = AGENT_CONFIG.read_text(encoding="utf-8")
    reference = COMMERCE_REFERENCE.read_text(encoding="utf-8")

    assert "单个任务直接生成一张完整合成图" in skill
    assert "`batchSize` 固定为 `1`" in reference
    assert "无法视觉检查" in reference
    assert "不得增加收费任务" in reference
    assert "`compose-layout`" in skill
    assert "成组版式先用一个任务直出完整合成图" in agent
