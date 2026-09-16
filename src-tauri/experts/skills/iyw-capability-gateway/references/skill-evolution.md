# Skill 使用与自进化

在实际使用 Skill 时主动检查触发、步骤、参数、输出和验证是否有效。
发现可复现错误、用户纠正或经验证的更好方法，就尝试一次最小改进。
正常成功且没有改进依据时不制造改动，也不增加无关任务。

## 先复用，再改进

1. 使用前按 Skill 名称和当前任务检索相关 Agent experience，复用有效经验。
2. 如果需要改进，读取实际加载的 Skill 和本次相关引用，核对版本及路径。
3. 用本参考旁的 `../scripts/skill-evolution.mjs list --skill <真实目录>`
   查看之前提议的模式、证据、接受和拒绝记录。不要重复已经失败的相同修改。
4. 完成本次用户任务并验证输出。区分 Skill 缺陷与环境故障；不要把一次服务
   超时改成全局禁用规则，或把用户一次性要求推广为所有任务的规则。
5. 有具体依据时，通过现有 `skill-creator` 编写一个原子改进，使用下述工具
   暂存、评估、应用。每次使用最多尝试一个改进，避免无限自我改写。

## 证据与状态

用户事实仍由 `manage_iyw_memory` 管理。六字段经验信封负责记录本次 Skill 名称、
问题、解决办法、结果、验证和复用条件。这里的脚本只管理 Skill 改进版本及
评估记录，不维护第二份用户记忆。

改进状态保存在真实 Skill 目录旁 `.iyw-skill-evolution/<路径摘要>/`，不会
修改已安装 Skill 的元数据或覆盖分发源。每个提议保留 baseline、candidate、
pattern、evidence、评估案例摘要和历史。`list` 是按需加载的模式索引，`show`
提供完整证据。拒绝和回滚只影响 Skill 版本，不清空学习记录。

```text
staged -> evaluated -> accepted -> rolled_back
       -> rejected
```

## 可执行流程

以下命令的脚本路径相对于本参考所属的 `iyw-capability-gateway` 根目录。
用会话已提供的共享 Node 路径运行。所有输入 JSON 用文件传入，不拼接 shell。

```text
node scripts/skill-evolution.mjs list --skill <skill-directory>
node scripts/skill-evolution.mjs stage --skill <skill-directory> --input <proposal.json>
node scripts/skill-evolution.mjs show --skill <skill-directory> --id <returned-id>
node scripts/skill-evolution.mjs evaluate --skill <skill-directory> --id <returned-id> --input <evaluation.json>
node scripts/skill-evolution.mjs apply --skill <skill-directory> --id <returned-id>
node scripts/skill-evolution.mjs rollback --skill <skill-directory> --id <returned-id>
```

`proposal.json` 包含 `target`（现有 Markdown 相对路径）、`content`（完整替换
内容）、`pattern`（稳定的根因或策略名称）、`evidence`（本次观测与验证）。
同样的 baseline/candidate 返回原提议，不能靠重复提交冲掉拒绝结果。
这版支持单个 Markdown 指令文件；脚本或多文件修改走项目原有代码审查流程。

`evaluation.json` 包含：

- `command`：可信评估器的绝对可执行路径及参数数组，原样执行，不通过 shell。
- `rubric`：预先确定的验收标准。
- `cases`：2 至 8 个 `{ "id": "...", "input": "..." }`，至少含当前缺陷和
  一个正常回归场景。两版用完全相同的输入、标准、模型与运行配置。

评估器在独立临时 Skill 副本中运行，每例最长 60 秒；环境变量
`IYW_EVAL_SKILL_DIR`、`IYW_EVAL_CASE`（JSON）、`IYW_EVAL_RUBRIC` 提供输入。
评估器必须真正运行或检查对应副本，stdout 仅输出 JSON：

```json
{"score":0.9,"passed":true,"evidence":"实际结果及执行过的验证"}
```

`score` 在 0 到 1 之间。超时、非零退出和无效结果都不能通过。门禁要求：
所有候选案例通过、每例不退化、至少一例严格改善。静态文字质量检查只能支持
文字结论；行为改进需要实际任务重放或现有评估器。禁止编造分数或只比较字数。
不具备可信评估条件时保留 staged，准确说明尚未验证，不声称已经进化。

## 应用与恢复

通过评估才可 apply。普通任务中的局部改进遵循用户既有授权，不重复索取
确认；全局规则、共享受管 Skill、依赖和外部操作遵循项目现有权限边界。
受版本管理或签名保护的 Skill，应通过既有维护入口更新，不能绕过分发管理。

应用前核对整个 Skill 的内容指纹，旧版本已经变化则保留提议并重新基于当前版本
评估。回滚也检查指纹，不能覆盖用户或其他 Agent 的后续改动。应用中断后可用
rollback 恢复原文件；接受、拒绝、回滚的原因和案例结果全部保留。

每次任务的回复保持简短。只有真正存储或应用成功后才说“已记住/已改进”；
用户询问学到了什么时，按相关经验和提议历史给出真实结果。

## 参考对应

以下仅记录历史设计来源，不是技能依赖或调用入口；不得据此加载、安装或恢复已废弃技能。

- `self-improving` 1.2.16：明确偏好与纠正、按需读取、分层管理。
- `self-improving-agent`：错误/知识缺口/最佳实践、稳定 Pattern-Key、记录解决结果。
- `skill-improver`：使用缺陷与临时绕过驱动修复，优先改触发和指令逻辑。
- WikiSkill、Wiki Garden、WikiEvolve：保留证据和失败提议，一次改一个行为，
  baseline/candidate 使用相同评估，回滚 Skill 不抹去经验。
- Comet：按任务加载，先试用，再根据真实采纳与验证结果积累可信度。
