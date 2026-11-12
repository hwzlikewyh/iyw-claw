export interface PluginPromptReference {
  label: string
  pluginSlug: string
}

export interface ParsedPluginPromptInstruction extends PluginPromptReference {
  remainder: string
}

const INSTRUCTION_PREFIX = "本次使用插件 "
const INSTRUCTION_SUFFIX = "，请通过当前会话已授权的该插件能力处理以下任务。"

export function pluginPromptInstruction(plugin: PluginPromptReference): string {
  return `${INSTRUCTION_PREFIX}${plugin.label}（${plugin.pluginSlug}）${INSTRUCTION_SUFFIX}`
}

export function parsePluginPromptInstruction(
  value: string
): ParsedPluginPromptInstruction | null {
  const instruction = value.trim()
  if (!instruction.startsWith(INSTRUCTION_PREFIX)) return null
  const suffixIndex = instruction.indexOf(
    INSTRUCTION_SUFFIX,
    INSTRUCTION_PREFIX.length
  )
  if (suffixIndex < 0) return null
  const body = instruction.slice(INSTRUCTION_PREFIX.length, suffixIndex)
  const remainder = instruction.slice(suffixIndex + INSTRUCTION_SUFFIX.length)
  if (remainder && !/^\s/.test(remainder)) return null
  const separator = body.lastIndexOf("（")
  if (separator < 1 || !body.endsWith("）")) return null

  const label = body.slice(0, separator).trim()
  const pluginSlug = body.slice(separator + 1, -1)
  if (
    !label ||
    !/^[a-zA-Z0-9_-][a-zA-Z0-9._-]*$/.test(pluginSlug) ||
    pluginSlug.includes("..")
  )
    return null
  return { label, pluginSlug, remainder: remainder.trimStart() }
}
