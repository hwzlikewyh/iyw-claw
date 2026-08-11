# CRM 与励销凭据保留实施计划

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 仅在密码接口明确拒绝账号密码时清理保存密码，并禁止 Agent 再向客户索要凭据。

**Architecture:** 为两套客户端增加 `CredentialRejectedError`，让会话失效继续使用
`AuthenticationError` 触发单次自动登录，而保存凭据清理只捕获专用错误。Skill 文档和
安装副本同步采用相同契约，真实凭据仅由内部操作人首次提供并写入现有 JSON。

**Tech Stack:** Python 3.10+ 标准库、pytest、Markdown、YAML。

## Global Constraints

- 继续使用 `~/.iyw-claw/` 下现有明文 JSON，不新增依赖或系统专属凭据服务。
- 只有密码登录接口明确拒绝账号或密码时才删除保存密码。
- 验证码、TTOCR、SSO、Token、网络、超时和限流错误均保留密码。
- 不读取、打印或写入任何真实凭据到测试、日志、文档或 Git。
- 本机无凭据时只向当前内部操作人收集一次，禁止联系客户重新索要。
- 每条命令最多自动登录一次；读取请求最多重放一次；副作用请求不重放。
- 保留用户已有未跟踪文件；不执行 commit、push、tag 或 release，除非另行授权。

---

### Task 1: CRM 明确区分密码拒绝与登录链路异常

**Files:**
- Modify: `tests/test_crm_remembered_login.py`
- Modify: `iyw-crm-workflows/scripts/iyw_crm_client.py`
- Modify: `iyw-crm-workflows/scripts/iyw_crm.py`

**Interfaces:**
- Produces: `CredentialRejectedError(AuthenticationError)`，错误码仍为
  `authentication_required`。
- Consumes: `extract_login_message(html: str) -> str` 和现有
  `SessionStore.invalidate_saved_credentials()`。

- [ ] **Step 1: 写入失败测试**

```python
def test_saved_login_noncredential_auth_failure_preserves_password(tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    client.login.side_effect = AuthenticationError("verification token missing")

    with pytest.raises(AuthenticationError):
        iyw_crm._login_with_saved_credentials(client, store)

    assert store.saved_credentials() == ("saved-user", "saved-pass")


def test_saved_login_credential_rejection_invalidates_password(tmp_path):
    store = _saved_store(tmp_path)
    client = Mock()
    client.login.side_effect = CredentialRejectedError("用户名或密码错误")

    with pytest.raises(CredentialRejectedError):
        iyw_crm._login_with_saved_credentials(client, store)

    assert store.saved_credentials() == ("saved-user", None)
```

- [ ] **Step 2: 验证测试先失败**

Run: `uv run --no-project --with pytest pytest tests/test_crm_remembered_login.py -q`

Expected: 新增测试因缺少 `CredentialRejectedError` 或仍误删密码而失败。

- [ ] **Step 3: 实现最小错误分类**

```python
class CredentialRejectedError(AuthenticationError):
    pass


def _is_credential_rejection(message: str) -> bool:
    normalized = message.replace(" ", "")
    return any(
        marker in normalized
        for marker in ("用户名或密码", "账号或密码", "登录名或密码", "密码错误")
    )
```

`CrmClient.login()` 仅在 POST 后仍返回登录页且 `_is_credential_rejection(detail)` 为 true
时抛出专用错误；其他登录页异常继续抛出 `AuthenticationError`。
`_login_with_saved_credentials()` 只捕获 `CredentialRejectedError` 清理密码和 Cookie。

- [ ] **Step 4: 验证 CRM 定向测试通过**

Run: `uv run --no-project --with pytest pytest tests/test_crm_remembered_login.py -q`

Expected: 全部通过，非凭据认证异常仍保留 `saved-pass`。

---

### Task 2: 励销只让 password-login 拒绝清理密码

**Files:**
- Modify: `tests/test_lixiao_remembered_login.py`
- Modify: `lixiao-workflows/scripts/lixiao_client.py`
- Modify: `lixiao-workflows/scripts/lixiao.py`

**Interfaces:**
- Produces: `CredentialRejectedError(AuthenticationError)`。
- Changes: `LixiaoClient._open()` 和验证 helper 接收 `operation: str`，仅当
  `operation == "password-login"` 且结构化 UC 业务码为 401 时抛出专用错误。普通 HTTP
  401 缺少足够的凭据拒绝证据，继续使用 `AuthenticationError` 并保留密码。

- [ ] **Step 1: 写入失败测试**

```python
def test_saved_login_downstream_auth_failure_preserves_password(monkeypatch, tmp_path):
    store = _saved_store(tmp_path)
    monkeypatch.setattr(
        lixiao,
        "_auto_login",
        Mock(side_effect=AuthenticationError("CRM business token is empty")),
    )

    with pytest.raises(AuthenticationError):
        lixiao._login_with_saved_credentials(Mock(), store)

    assert store.saved_credentials() == ("saved-phone", "saved-pass")


def test_saved_login_credential_rejection_invalidates_password(monkeypatch, tmp_path):
    store = _saved_store(tmp_path)
    monkeypatch.setattr(
        lixiao,
        "_auto_login",
        Mock(side_effect=CredentialRejectedError("password rejected")),
    )

    with pytest.raises(CredentialRejectedError):
        lixiao._login_with_saved_credentials(Mock(), store)

    assert store.saved_credentials() == ("saved-phone", None)
```

同时补客户端响应分类测试：`password-login` 的结构化 UC 401 为
`CredentialRejectedError`，`app-session` 的 UC 401 和普通 HTTP 401 仍为
`AuthenticationError`。

- [ ] **Step 2: 验证测试先失败**

Run: `uv run --project lixiao-workflows --with pytest pytest tests/test_lixiao_remembered_login.py -q`

Expected: 新增测试因通用 `AuthenticationError` 仍触发凭据清理而失败。

- [ ] **Step 3: 实现操作级错误分类**

```python
class CredentialRejectedError(AuthenticationError):
    pass


def _authentication_error(operation: str, message: str) -> AuthenticationError:
    error_type = (
        CredentialRejectedError
        if operation == "password-login"
        else AuthenticationError
    )
    return error_type(message)
```

将 `call.operation` 从 `execute()` 传入 `_open()`、`_validate_response()`、
`_validate_uc_result()` 和 `_raise_http_error()`；`lixiao.py` 只捕获
`CredentialRejectedError` 调用 `invalidate_saved_credentials()`。结构化 UC 校验必须先于
通用 `success: false` 校验，且只精确匹配业务码 `401`。`password-login` 成功后立即保存
phone/password，再继续 CRM SSO 和 app-session，使下游故障不会丢失已验证凭据。

- [ ] **Step 4: 验证励销定向测试通过**

Run: `uv run --project lixiao-workflows --with pytest pytest tests/test_lixiao_remembered_login.py -q`

Expected: 全部通过，下游 SSO/Token 认证异常保留 `saved-pass`。

---

### Task 3: 固化禁止向客户索要的 Skill 契约

**Files:**
- Modify: `iyw-crm-workflows/SKILL.md`
- Modify: `iyw-crm-workflows/references/commands.md`
- Modify: `iyw-crm-workflows/agents/openai.yaml`
- Modify: `lixiao-workflows/SKILL.md`
- Modify: `lixiao-workflows/references/commands.md`
- Modify: `lixiao-workflows/agents/openai.yaml`
- Modify: `tests/test_auth_skill_docs.py`

**Interfaces:**
- Produces: 两套 Skill 一致的内部凭据恢复规则，不改变 CLI 参数。

- [ ] **Step 1: 添加文档契约失败测试**

```python
def test_auth_skills_never_request_credentials_from_customers():
    for path in ("iyw-crm-workflows/SKILL.md", "lixiao-workflows/SKILL.md"):
        skill = _read(path)
        assert "当前内部操作人" in skill
        assert "禁止联系客户" in skill
```

- [ ] **Step 2: 更新 Skill、命令参考和默认提示**

明确写入：先检查 `auth status`；非凭据错误保留密码且不得提问；本机确实缺少完整凭据时
只向当前内部操作人逐项收集一次并立即登录持久化；禁止联系客户或建议找客户重新索要。

- [ ] **Step 3: 验证文档和 Skill 结构**

Run: `uv run --no-project --with pytest pytest tests/test_auth_skill_docs.py -q`

Run: `uv run --no-project python C:/Users/iyw/.codex/skills/.system/skill-creator/scripts/quick_validate.py iyw-crm-workflows`

Run: `uv run --no-project python C:/Users/iyw/.codex/skills/.system/skill-creator/scripts/quick_validate.py lixiao-workflows`

Expected: 文档测试与两个 Skill 校验全部通过。

---

### Task 4: 回归验证并同步当前安装副本

**Files:**
- Sync: `iyw-crm-workflows/` -> `C:/Users/iyw/.iyw-claw/skills/iyw-crm-workflows/`
- Sync: `lixiao-workflows/` -> `C:/Users/iyw/.iyw-claw/skills/lixiao-workflows/`

**Interfaces:**
- Consumes: Task 1-3 的仓库实现。
- Produces: 当前 IYW Claw 安装副本与仓库 Skill 内容一致。

- [ ] **Step 1: 运行认证回归测试**

Run: `uv run --no-project --with pytest pytest tests/test_crm_remembered_login.py tests/test_lixiao_remembered_login.py tests/test_auth_skill_docs.py -q`

Expected: 全部通过。

- [ ] **Step 2: 同步两个 Skill 目录**

使用精确源目录覆盖当前安装副本，保留 `~/.iyw-claw/credentials.json` 和
`~/.iyw-claw/iyw-crm-workflows/session.json`，不得删除或读取凭据文件。

- [ ] **Step 3: 核对安装文件哈希**

对仓库与安装副本的 `SKILL.md`、`agents/openai.yaml`、`references/commands.md` 和
`scripts/*.py` 计算 SHA-256；对应文件必须完全一致。

- [ ] **Step 4: 检查非敏感状态与工作区**

Run: `uv run --no-project python C:/Users/iyw/.iyw-claw/skills/iyw-crm-workflows/scripts/iyw_crm.py auth status`

Run: `uv run --no-project python C:/Users/iyw/.iyw-claw/skills/lixiao-workflows/scripts/lixiao.py auth status`

Expected: 命令只显示路径、布尔状态和 Cookie 数量，不显示账号、密码或 Token。当前本机
仍无完整凭据时如实报告，由内部操作人后续录入一次；不得向客户索要。

Run: `git diff --check` and `git status --short`

Expected: 仅显示本任务修改及用户原有未跟踪文件，无测试缓存或临时凭据残留。
