from pathlib import Path


SKILL = Path(__file__).parents[1] / "iyw-image-workflows" / "SKILL.md"
AGENT_CONFIG = Path(__file__).parents[1] / "iyw-image-workflows" / "agents" / "openai.yaml"


def test_skill_documents_image_priority_and_fixed_tools():
    text = SKILL.read_text(encoding="utf-8")
    assert "指定图片、上传图片或提供图片 URL" in text
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
