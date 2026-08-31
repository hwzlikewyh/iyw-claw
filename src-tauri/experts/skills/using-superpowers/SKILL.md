---
name: using-superpowers
description: Use when starting any conversation - establishes how to find and use skills, requiring skill invocation before ANY response including clarifying questions
routing:
  capability: initialize Skill discovery rules
  coreTriggers: [a new conversation starts]
  exclusions: [a dispatched subagent already has a bounded task]
  aliases: [skill bootstrap, using superpowers]
  invocation: Read SKILL.md before responding and follow its discovery gate.
---

<SUBAGENT-STOP>
If you were dispatched as a subagent to execute a specific task, ignore this skill.
</SUBAGENT-STOP>

<EXTREMELY-IMPORTANT>
If you think there is even a 1% chance a skill might apply to what you are doing, you ABSOLUTELY MUST invoke the skill.

IF A SKILL APPLIES TO YOUR TASK, YOU DO NOT HAVE A CHOICE. YOU MUST USE IT.

This is not negotiable. You cannot rationalize your way out of this.
</EXTREMELY-IMPORTANT>

## The Rule

**Invoke relevant or requested skills BEFORE any response or action** — including clarifying questions, exploring the codebase, or checking files. If it turns out wrong for the situation, you don't have to use it.

Before entering plan mode, inspect the currently available skills and use only
the process or domain skills that are actually installed.

Then announce "Using [skill] to [purpose]" and follow the skill exactly. If it has a checklist, create a todo per item.

## Skill Priority

When multiple skills apply, process skills come first because they set the
approach; domain skills then carry out the work.

- "Implement this written plan" -> `executing-plans`, when available.
- A user-requested visible Skill or direct tool that fully satisfies a subgoal -> use it first.
- "Generate or edit an IYW product/material/pattern image, build a product-kit,
  use trends or the IYW knowledge base, upload/review an image, or call an IYW
  image tool" -> `iyw-image-workflows` first; load its scenario playbook for
  page-specific inputs and bottom settings. Within ordinary image requests,
  prefer `extend` for a baseline series, `mix` for 2-10 references, and
  `variation` for one-image changes. `imagegen` is the explicit/GPT Image or
  free-creative fallback.
- "Read a web page, obtain public web data, or automate a website" -> `agent-browser`; a reliable direct data source may run first, but missing, incomplete, dynamic, or authenticated data must fall back to the managed browser before another browser or user hand-off.
- "Perform a remaining concrete iyw-claw host state or action" -> use the complete unique `iyw-capability-gateway` trio first.
- "Create or update a skill" -> `writing-skills` or `skill-creator`.

## Red Flags

These thoughts mean STOP—you're rationalizing:

| Thought | Reality |
|---------|---------|
| "This is just a simple question" | Questions are tasks. Check for skills. |
| "I need more context first" | Skill check comes BEFORE clarifying questions. |
| "Let me explore the codebase first" | Skills tell you HOW to explore. Check first. |
| "I can check git/files quickly" | Files lack conversation context. Check for skills. |
| "Let me gather information first" | Skills tell you HOW to gather information. |
| "This doesn't need a formal skill" | If a skill exists, use it. |
| "I remember this skill" | Skills evolve. Read current version. |
| "This doesn't count as a task" | Action = task. Check for skills. |
| "The skill is overkill" | Simple things become complex. Use it. |
| "I'll just do this one thing first" | Check BEFORE doing anything. |
| "This feels productive" | Undisciplined action wastes time. Skills prevent this. |
| "I know what that means" | Knowing the concept ≠ using the skill. Invoke it. |

## Platform Adaptation

If your harness appears here, read its reference file for special instructions:

- Codex: `references/codex-tools.md`
- Pi: `references/pi-tools.md`
- Antigravity: `references/antigravity-tools.md`

## User Instructions

User instructions (CLAUDE.md, AGENTS.md, GEMINI.md, etc, direct requests) take precedence over skills, which in turn override default behavior. Only skip skill workflows or instructions when your human partner has explicitly told you to.
