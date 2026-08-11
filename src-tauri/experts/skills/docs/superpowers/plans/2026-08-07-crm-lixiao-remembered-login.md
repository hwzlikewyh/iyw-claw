# CRM 与励销记住账号密码实施计划

**Goal:** 让 `iyw-crm-workflows` 和 `lixiao-workflows` 跨平台明文记住账号密码，会话失效后自动登录一次，失败后清理失效密码并交由 Agent 重新询问。

**Architecture:** 两套 Skill 保持各自现有 JSON 文件。存储层增加定向删除和非敏感状态；CLI 编排层负责一次性自动登录及只读请求重放；底层 HTTP 客户端继续只执行单次请求。励销工作流执行前预检会话，副作用操作不在认证失败后重放。

**Tech Stack:** Python 标准库、urllib、pytest、Markdown/YAML Skill 文档。

## Global Constraints

- 不引入 DPAPI、keyring 或第三方依赖。
- 不输出真实账号、密码、Cookie 或 Token。
- dry-run 不读取凭据、不访问网络、不清理文件。
- 每条命令最多自动提交一次保存密码。
- 保留当前未跟踪文件，不执行 Git commit、push 或 tag。

### Task 1: CRM 凭据存储与一次性重登

**Files:**
- Modify: `iyw-crm-workflows/scripts/iyw_crm_config.py`
- Modify: `iyw-crm-workflows/scripts/iyw_crm_client.py`
- Modify: `iyw-crm-workflows/scripts/iyw_crm.py`
- Create: `tests/test_crm_remembered_login.py`

- [ ] 为 `SessionStore` 增加 `password` 字段、定向删除、保存凭据读取和两个非敏感状态。
- [ ] 登录完整成功后保存 password；会话刷新不得覆盖或输出 password。
- [ ] 支持只传 `--password` 复用保存用户名。
- [ ] `auth ensure` 和 `customer-search` 遇到 `AuthenticationError` 后自动登录一次；登录认证失败时清理 password/cookies。
- [ ] 保证 dry-run 先于 status/ensure/login 分支，不读取本地文件。
- [ ] 覆盖有效会话、成功恢复、错误密码、无密码、网络错误、单次重放和 dry-run 测试。

### Task 2: 励销定向失效与统一恢复边界

**Files:**
- Modify: `lixiao-workflows/scripts/lixiao_config.py`
- Modify: `lixiao-workflows/scripts/lixiao.py`
- Create: `tests/test_lixiao_remembered_login.py`

- [ ] 为 `CredentialStore` 增加定向删除和 `has_saved_account`/`has_saved_credentials`，保留 `has_account` 兼容字段。
- [ ] 支持只传 `--password` 复用保存 phone。
- [ ] 收敛保存凭据自动登录为一次性 helper；只有 `AuthenticationError` 清理 password 和用户会话。
- [ ] 读取类 API 登录成功后只重放一次；`company-unlock` 执行前预检且认证失败后不重放。
- [ ] 非 dry-run workflow 开始前自动 ensure，一旦中途认证失败不整体重放。
- [ ] dry-run/status/list 不触发自动登录，dry-run 不加载凭据。
- [ ] 覆盖旧明文文件兼容、字段保留、成功恢复、错误密码、验证码/网络错误、副作用保护和工作流预检测试。

### Task 3: 更新两套 Skill 契约

**Files:**
- Modify: `iyw-crm-workflows/SKILL.md`
- Modify: `iyw-crm-workflows/agents/openai.yaml`
- Modify: `iyw-crm-workflows/references/commands.md`
- Modify: `lixiao-workflows/SKILL.md`
- Modify: `lixiao-workflows/agents/openai.yaml`
- Modify: `lixiao-workflows/references/commands.md`
- Modify: `tests/test_auth_skill_docs.py`

- [ ] 明确默认自动登录一次，失败后才询问用户。
- [ ] 有保存账号时只问密码；账号也缺失时继续分两次询问。
- [ ] 明确明文存储路径、logout 清理范围、网络/验证码错误不清理密码。
- [ ] 锁定 Agent 提问和 CLI 调用文档契约。

### Task 4: 验证与收尾

- [ ] 运行新增认证测试和现有认证文档测试。
- [ ] 使用 `quick_validate.py` 校验两个 Skill。
- [ ] 运行完整 pytest 测试套件。
- [ ] 运行 `git diff --check` 并确认只修改本任务文件；不清理用户已有未跟踪文件。
