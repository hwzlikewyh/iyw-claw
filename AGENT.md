# Agent 规则

## 智能体名称

所有智能体**必须使用中文名称**，禁止在 UI、日志、提示文字、文档中出现原始英文名称。

| 类型键 | 中文名 | 禁止使用的原名 |
|---|---|---|
| `claude_code` | 远山 | Claude Code、claude-code |
| `codex` | 星河 | Codex |
| `open_code` | 云舟 | OpenCode、open-code |
| `gemini` | 流光 | Gemini CLI、gemini |
| `open_claw` | 开放之爪 | OpenClaw、open-claw |
| `cline` | 逐风 | Cline |
| `hermes` | 赫尔墨斯 | Hermes |
| `code_buddy` | 青岚 | Code Buddy、code-buddy |
| `kimi_code` | 月白 | Kimi Code、kimi-code |
| `pi` | 墨川 | Pi |
| `grok` | 知微 | Grok |

权威映射定义在 `src/lib/types.ts` 的 `AGENT_LABELS`，所有展示层统一通过 `getAgentDisplayName()` 获取中文名。
