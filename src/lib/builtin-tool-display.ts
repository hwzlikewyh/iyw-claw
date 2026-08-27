const GATEWAY_TOOL_NAMES = [
  "search_iyw_capabilities",
  "read_iyw_capability",
  "invoke_iyw_capability",
] as const

const CAPABILITY_ID_TO_TOOL: Readonly<Record<string, string>> = {
  "iyw.automation.projects.list.v1": "list_scheduled_task_projects",
  "iyw.automation.tasks.list.v1": "list_scheduled_tasks",
  "iyw.automation.tasks.create.v1": "create_scheduled_task",
  "iyw.automation.tasks.update.v1": "update_scheduled_task",
  "iyw.automation.tasks.delete.v1": "delete_scheduled_task",
  "iyw.browser.tabs.list.v1": "browser_list_tabs",
  "iyw.browser.page.open.v1": "browser_open",
  "iyw.browser.page.snapshot.v1": "browser_snapshot",
  "iyw.browser.page.read.v1": "browser_read",
  "iyw.browser.element.click.v1": "browser_click",
  "iyw.browser.element.fill.v1": "browser_fill",
  "iyw.browser.keyboard.press.v1": "browser_press",
  "iyw.browser.page.scroll.v1": "browser_scroll",
  "iyw.browser.page.wait.v1": "browser_wait",
  "iyw.browser.page.screenshot.v1": "browser_screenshot",
  "iyw.browser.tabs.close.v1": "browser_close_tab",
  "iyw.browser.command.run.v1": "browser_command",
  "iyw.browser.user_action.request.v1": "browser_request_user_action",
  "iyw.browser.window.present.v1": "browser_present",
  "iyw.browser.window.close.v1": "browser_close_window",
  "iyw.artifacts.present.v1": "present_task_files",
  "iyw.delegation.tasks.create.v1": "delegate_to_agent",
  "iyw.delegation.tasks.read.v1": "get_delegation_status",
  "iyw.delegation.tasks.cancel.v1": "cancel_delegation",
  "iyw.interaction.feedback.read.v1": "check_user_feedback",
  "iyw.interaction.question.ask.v1": "ask_user_question",
  "iyw.session.info.read.v1": "get_session_info",
  "iyw.audio.transcription.create.v1": "transcribe_audio",
  "iyw.audio.transcription.flash.create.v1": "transcribe_audio_flash",
  "iyw.audio.transcription.read.v1": "query_audio_transcription",
  "iyw.image.present.v1": "show_image",
  "iyw.image.analyze.v1": "analyze_image",
  "iyw.session.user_profile.read.v1": "get_current_user_profile",
  "iyw.memory.confirmed.append.v1": "append_user_memory",
  "iyw.memory.candidate.propose.v1": "propose_user_memory",
  "iyw.memory.recall.search.v1": "memory_recall",
  "iyw.memory.documents.read.v1": "read_user_memory_documents",
  "iyw.channels.list.v1": "list_message_channels",
  "iyw.channels.save.v1": "save_message_channel",
  "iyw.channels.delete.v1": "delete_message_channel",
  "iyw.channels.credentials.manage.v1": "manage_channel_credential",
  "iyw.channels.operate.v1": "operate_message_channel",
  "iyw.channels.targets.list.v1": "list_channel_targets",
  "iyw.channels.messages.list.v1": "list_channel_messages",
  "iyw.channels.messages.send.v1": "send_channel_messages",
  "iyw.channels.settings.manage.v1": "manage_channel_settings",
}

const CAPABILITY_TOOL_NAMES = new Set(Object.values(CAPABILITY_ID_TO_TOOL))

const NESTED_INPUT_KEYS = ["input", "arguments", "params", "payload"]

export interface BuiltinToolDisplay {
  /** Translation key under `Folder.chat.contentParts.builtinTool`. */
  toolName: string
  /** Internal capability inputs are implementation details, not user content. */
  hideInput: true
}

function canonicalName(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
}

function parseJson(value: string): unknown {
  try {
    return JSON.parse(value)
  } catch {
    return null
  }
}

function findStringField(
  value: unknown,
  key: string,
  depth: number = 0
): string | null {
  if (depth > 4 || value == null) return null

  if (typeof value === "string") {
    const parsed = parseJson(value)
    return parsed == null ? null : findStringField(parsed, key, depth + 1)
  }

  if (Array.isArray(value)) {
    for (const item of value) {
      const found = findStringField(item, key, depth + 1)
      if (found) return found
    }
    return null
  }

  if (typeof value !== "object") return null
  const record = value as Record<string, unknown>
  const direct = record[key]
  if (typeof direct === "string" && direct.trim()) return direct.trim()

  for (const nestedKey of NESTED_INPUT_KEYS) {
    const found = findStringField(record[nestedKey], key, depth + 1)
    if (found) return found
  }
  return null
}

function matchKnownToolName(toolName: string): string | null {
  const canonical = canonicalName(toolName)
  if (CAPABILITY_TOOL_NAMES.has(canonical)) return canonical
  if (
    GATEWAY_TOOL_NAMES.includes(
      canonical as (typeof GATEWAY_TOOL_NAMES)[number]
    )
  ) {
    return canonical
  }

  const known = [...CAPABILITY_TOOL_NAMES, ...GATEWAY_TOOL_NAMES]
  return known.find((name) => canonical.endsWith(`_${name}`)) ?? null
}

/**
 * Resolve an internal MCP call to a stable, localizable display name.
 *
 * Gateway calls may arrive under a server-prefixed name, while invoke calls
 * may already be rewritten to the logical tool name by the ACP layer. The
 * capability id is authoritative whenever it is present in the input.
 */
export function getBuiltinToolDisplay(
  toolName: string,
  input: string | null | undefined
): BuiltinToolDisplay | null {
  const capabilityId = input ? findStringField(input, "capability_id") : null
  const resolvedTool = capabilityId
    ? CAPABILITY_ID_TO_TOOL[capabilityId]
    : undefined
  const matchedTool = resolvedTool ?? matchKnownToolName(toolName)
  if (!matchedTool) return null

  return {
    toolName: matchedTool,
    hideInput: true,
  }
}
