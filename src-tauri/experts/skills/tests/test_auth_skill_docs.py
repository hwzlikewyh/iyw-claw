from pathlib import Path


ROOT = Path(__file__).parents[1]


def _read(relative_path: str) -> str:
    return (ROOT / relative_path).read_text(encoding="utf-8")


def test_crm_credentials_are_requested_one_at_a_time():
    skill = _read("iyw-crm-workflows/SKILL.md")
    agent = _read("iyw-crm-workflows/agents/openai.yaml")

    assert "必须使用 `ask_user_question` 分两次提问" in skill
    assert "第一次只问 IYW CRM 账号并等待用户回答" in skill
    assert "第二次只问 IYW CRM 密码并等待用户回答" in skill
    assert "Never request both values in one call or one question" in agent


def test_lixiao_credentials_are_requested_one_at_a_time():
    skill = _read("lixiao-workflows/SKILL.md")
    agent = _read("lixiao-workflows/agents/openai.yaml")

    assert "call `ask_user_question` once to ask only for the Lixiao account" in skill
    assert "call `ask_user_question` a second time" in skill
    assert "Never request both values in one call or one question" in agent
