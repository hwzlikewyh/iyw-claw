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

type TranslateToolLabel = (
  key: string,
  values?: Record<string, unknown>
) => string | null

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
const COMMAND_PREFIX =
  /^(?:git|npm|pnpm|yarn|node|python|cargo|curl|wget|powershell|pwsh|cmd|bash|sh)\b/i
const ERROR_PREFIX =
  /^(?:error|failed|failure|exception|traceback|fatal|panic|错误|失败|异常)(?:\s|[:：]|$)/i
const ERROR_DETAIL =
  /(?:^|\s)(?:\d{3}\s+bad gateway|bad gateway|connection not found|network error|request (?:was )?rejected|timed? out|unauthorized|forbidden)(?:\s|[:：.]|$)/i
const UUID_PATTERN =
  /(?:^|\s)[a-f0-9]{8}-[a-f0-9]{4}-[1-5a-f0-9][a-f0-9]{3}-[89abf0-9][a-f0-9]{3}-[a-f0-9]{12}(?:\s|$)/i

export function getReadableToolActivityTitle(
  value: string | null | undefined
): string | null {
  if (!value || /[\u0000-\u001f\u007f]/.test(value)) return null
  const title = value.replace(/\s+/g, " ").trim()
  if (!title || Array.from(title).length > MAX_ALIAS_LENGTH) return null
  const naturalLanguage =
    /[\u3400-\u9fff]/.test(title) || /[a-z]{2,}\s+[a-z]{2,}/i.test(title)
  if (
    !naturalLanguage ||
    /[/\\_:：`{}[\]=|;&<>]|\$[({]|https?:|\.[a-z0-9]{1,8}(?:\s|$)/i.test(
      title
    ) ||
    /(?:^|\s)-{1,2}[a-z0-9]|(?:^|\s)%[^%\s]+%/i.test(title) ||
    /(?:^|\s)[a-f0-9]{24,}(?:\s|$)/i.test(title) ||
    /\bserver\b/i.test(title) ||
    UUID_PATTERN.test(title) ||
    COMMAND_PREFIX.test(title) ||
    ERROR_PREFIX.test(title) ||
    ERROR_DETAIL.test(title)
  )
    return null
  return (
    title.replace(/^(?:原助理\s*)?(?:正在|准备|即将)\s*/, "").trim() || null
  )
}

function readableAlias(value: string | null | undefined): string | null {
  return getReadableToolActivityTitle(value)
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
  const description = getReadableToolActivityTitle(
    getIywToolDescription(tool.toolName, tool.input)
  )
  if (description) return description
  const builtin = getBuiltinToolDisplay(
    tool.toolName,
    tool.input,
    tool.displayTitle
  )
  const fallbackName = getReadableToolActivityTitle(builtin?.fallbackName)
  if (builtin && fallbackName) {
    if (builtin.toolName === "read_iyw_capability") {
      const label = translate("builtinTool.read_iyw_capability")
      return label ? `${label}: ${fallbackName}` : fallbackName
    }
    return (
      translate("builtinTool.dynamicCapability", {
        name: fallbackName,
      }) ?? fallbackName
    )
  }
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
