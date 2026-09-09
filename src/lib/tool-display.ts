import {
  getBuiltinToolDisplay,
  getIywToolDescription,
} from "@/lib/builtin-tool-display"
import { normalizeToolName } from "@/lib/tool-call-normalization"

const TOOL_ACTIONS: Readonly<Record<string, string>> = {
  bash: "command",
  exec_command: "command",
  shell: "command",
  read: "read",
  edit: "edit",
  apply_patch: "edit",
  write: "write",
  notebookedit: "edit",
  glob: "findFiles",
  grep: "searchFiles",
  webfetch: "fetch",
  websearch: "searchWeb",
  todowrite: "plan",
  taskcreate: "plan",
  taskupdate: "plan",
  tasklist: "plan",
  update_plan: "plan",
  enterplanmode: "plan",
  exitplanmode: "plan",
  switch_mode: "plan",
  create_goal: "plan",
  update_goal: "plan",
  agent: "delegate",
  task: "task",
  skill: "skill",
  question: "question",
  request_user_input_async: "question",
  lsp: "inspectCode",
  attempt_completion: "finish",
  view_image: "viewImage",
  imagegen: "generateImage",
  exec: "tool",
  wait: "wait",
  write_stdin: "wait",
  wait_agent: "wait",
}

const GENERIC_ACTIONS: ReadonlyArray<readonly [RegExp, string]> = [
  [/(?:^|_)(?:search|find|query)(?:_|$)/, "search"],
  [/(?:^|_)(?:list|get|read|fetch)(?:_|$)/, "readData"],
  [/(?:^|_)(?:create|add)(?:_|$)/, "createData"],
  [/(?:^|_)(?:update|edit|set|save)(?:_|$)/, "updateData"],
  [/(?:^|_)(?:delete|remove)(?:_|$)/, "deleteData"],
  [/(?:^|_)(?:send|reply)(?:_|$)/, "send"],
  [/(?:^|_)(?:wait|poll)(?:_|$)/, "wait"],
]

type TranslateToolLabel = (key: string) => string | null

interface ToolDisplayInput {
  toolName: string
  input?: string | null
  displayTitle?: string | null
}

function canonicalName(name: string): string {
  return name
    .replace(/([a-z])([A-Z])/g, "$1_$2")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
}

function translatedTool(name: string, translate: TranslateToolLabel) {
  const tokens = canonicalName(name).split("_")
  for (let index = 0; index < tokens.length; index += 1) {
    const label = translate(`builtinTool.${tokens.slice(index).join("_")}`)
    if (label) return label
  }
  return null
}

const MAX_ALIAS_LENGTH = 64

function readableAlias(value: string | null | undefined): string | null {
  const alias = value?.trim()
  if (!alias || alias.length > MAX_ALIAS_LENGTH) return null
  // 标题仅接受自然语言，路径、命名空间和命令留在展开的详情里。
  if (
    !/[\u3400-\u9fff]/.test(alias) ||
    /[/\\_:：`\n{}=]|\.[a-z0-9]/i.test(alias)
  )
    return null
  return alias.replace(/^(?:原助理\s*)?(?:正在|准备|即将)\s*/, "") || null
}

function bareToolName(name: string): string {
  return (
    name
      .split(/__|[/:.]/)
      .pop()
      ?.trim() ?? ""
  )
}

export function getToolDisplayName(
  tool: ToolDisplayInput,
  translate: TranslateToolLabel
): string {
  const description = getIywToolDescription(tool.toolName, tool.input)
  if (description) return description
  const builtin = getBuiltinToolDisplay(tool.toolName, tool.input)
  const builtinLabel = builtin && translate(`builtinTool.${builtin.toolName}`)
  if (builtinLabel) return builtinLabel
  const known =
    translatedTool(tool.toolName, translate) ??
    translatedTool(tool.displayTitle ?? "", translate)
  if (known) return known
  const bareName = bareToolName(tool.toolName)
  const titleName = bareToolName(tool.displayTitle ?? "")
  const alias =
    readableAlias(tool.displayTitle) ??
    readableAlias(tool.toolName) ??
    readableAlias(bareName)
  if (alias) return alias
  const action =
    TOOL_ACTIONS[canonicalName(titleName)] ??
    TOOL_ACTIONS[canonicalName(bareName)] ??
    TOOL_ACTIONS[normalizeToolName(tool.toolName).toLowerCase()]
  if (action) return translate(`toolAction.${action}`) ?? ""
  const generic = GENERIC_ACTIONS.find(([pattern]) =>
    pattern.test(canonicalName(bareName))
  )?.[1]
  return translate(`toolAction.${generic ?? "tool"}`) ?? ""
}
