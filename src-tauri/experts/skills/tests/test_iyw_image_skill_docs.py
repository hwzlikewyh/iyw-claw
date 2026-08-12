from pathlib import Path

SKILL = Path(__file__).parents[1] / "iyw-image-workflows" / "SKILL.md"
AGENT_CONFIG = (
    Path(__file__).parents[1] / "iyw-image-workflows" / "agents" / "openai.yaml"
)
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
    assert "`example <alias>`" in text
    assert "`search <alias> --input-file <path> --dry-run`" in text
    assert "帮助输出" not in text
    assert "该别名内部调用 `g_tools_generate_image`" in text


def test_search_contract_reference_lists_all_aliases_and_safe_flow():
    text = (
        Path(__file__).parents[1]
        / "iyw-image-workflows"
        / "references"
        / "tool-contracts.md"
    ).read_text(encoding="utf-8")
    for alias in (
        "image",
        "catalog",
        "dict-industry",
        "report-areas",
        "report-years",
        "report-list",
        "report-detail",
        "report-detail-tu",
        "report-recommendations",
        "report-images",
        "report-full",
        "trend-dict",
        "tool-config",
        "trend-list",
        "trend-detail",
        "ip-list",
        "ip-patterns",
    ):
        assert f"`{alias}`" in text
    assert "example image" in text
    assert "签名 URL" in text
    assert "invalid_response" in text


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


def test_smart_search_theme_design_has_complete_decision_flow():
    skill = SKILL.read_text(encoding="utf-8")
    agent = AGENT_CONFIG.read_text(encoding="utf-8")
    reference = COMMERCE_REFERENCE.read_text(encoding="utf-8")

    assert "使用智能搜索获取趋势或主题素材并设计" in skill.split("---", 2)[1]
    assert "把智能搜索作为必需前置步骤" in skill
    assert "4 宫格或 6 宫格系列套组" in skill
    assert "优先推荐系列作品或" in skill
    assert "不得替用户静默决定" in skill
    assert "每个主题只有一页" in skill and "`tool variation`" in skill
    assert "每个主题有多页" in skill and "`tool mix`" in skill
    assert "选择系列结果后再使用 `extend` 延展" in skill
    assert "不得改用其他模型通道" in skill

    assert "`classify` 使用 `[51]`" in reference
    assert "每个主题独立判断页数" in reference
    assert "用户已明确结果形态及产品时不要重复询问" in reference
    assert "多页主题超过 10 页" in reference
    assert "payload 都固定使用 `modelChannel: 2`" in reference
    assert "不得退化为凭空生成" in reference

    assert "先搜索并推荐趋势或主题" in agent
    assert "生成前询问系列产品、4/6 宫格系列套组或单品/系列企划案" in agent
    assert "种子作品再用 modelChannel 2 的 extend 做系列延伸" in agent
