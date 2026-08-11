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


def test_crm_reuses_saved_account_after_one_automatic_login():
    skill = _read("iyw-crm-workflows/SKILL.md")
    commands = _read("iyw-crm-workflows/references/commands.md")
    agent = _read("iyw-crm-workflows/agents/openai.yaml")

    assert "自动登录一次" in skill
    assert "明文保存用户名、密码和 Cookie" in skill
    assert "auth login --password <CRM密码>" in skill
    assert "has_saved_account" in commands
    assert "has_saved_credentials" in commands
    assert "必须串行" in commands
    assert "automatically submits saved credentials at most once" in agent
    assert "auth login --password <password>" in agent


def test_lixiao_credentials_are_requested_one_at_a_time():
    skill = _read("lixiao-workflows/SKILL.md")
    agent = _read("lixiao-workflows/agents/openai.yaml")

    assert "call `ask_user_question` once to ask only for the Lixiao account" in skill
    assert "call `ask_user_question` a second time" in skill
    assert "Never request both values in one call or one question" in agent


def test_lixiao_reuses_saved_account_after_one_automatic_login():
    skill = _read("lixiao-workflows/SKILL.md")
    commands = _read("lixiao-workflows/references/commands.md")
    agent = _read("lixiao-workflows/agents/openai.yaml")

    assert "automatically submits\n  saved credentials once" in skill
    assert "plaintext" in skill
    assert "auth login --password <励销密码>" in skill
    assert "has_saved_account" in commands
    assert "has_saved_credentials" in commands
    assert "Serialize authentication and credential writes" in commands
    assert "automatically submits saved credentials at most once" in agent
    assert "auth login --password <password>" in agent


def test_auth_skills_never_request_credentials_from_customers():
    crm_skill = _read("iyw-crm-workflows/SKILL.md")
    crm_commands = _read("iyw-crm-workflows/references/commands.md")
    crm_agent = _read("iyw-crm-workflows/agents/openai.yaml")
    lixiao_skill = _read("lixiao-workflows/SKILL.md")
    lixiao_commands = _read("lixiao-workflows/references/commands.md")
    lixiao_agent = _read("lixiao-workflows/agents/openai.yaml")

    assert "当前内部操作人" in crm_skill
    assert "禁止联系客户" in crm_skill
    assert "禁止联系客户" in crm_commands
    assert "Never contact the customer" in crm_agent
    assert "current internal operator" in lixiao_skill
    assert "Never contact the customer" in lixiao_skill
    assert "Never contact the customer" in lixiao_commands
    assert "Never contact the customer" in lixiao_agent
