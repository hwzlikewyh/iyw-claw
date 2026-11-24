export interface PluginPromptReference {
  label: string
  pluginSlug: string
}

export interface ParsedPluginPromptInstruction extends PluginPromptReference {
  remainder: string
}

const INSTRUCTION_PREFIX = "本次使用插件 "
const INSTRUCTION_SUFFIX = "，请通过当前会话已授权的该插件能力处理以下任务。"
// 兼容历史消息中的半角括号、标点及合并文本时的空白。
const INSTRUCTION_PATTERN =
  /本次使用插件\s+([^\r\n]+?)[（(]\s*([a-zA-Z0-9_-][a-zA-Z0-9._-]*)\s*[）)]\s*[，,]\s*请通过当前会话已授权的该插件能力处理以下任务。/g

export function pluginPromptInstruction(plugin: PluginPromptReference): string {
  return `${INSTRUCTION_PREFIX}${plugin.label}（${plugin.pluginSlug}）${INSTRUCTION_SUFFIX}`
}

export function parsePluginPromptInstruction(
  value: string
): ParsedPluginPromptInstruction | null {
  const [instruction] = findPluginPromptInstructions(value)
  if (!instruction || value.slice(0, instruction.start).trim()) return null
  return {
    label: instruction.label,
    pluginSlug: instruction.pluginSlug,
    remainder: value.slice(instruction.end).trim(),
  }
}

export function findPluginPromptInstructions(value: string) {
  return Array.from(value.matchAll(INSTRUCTION_PATTERN)).flatMap((match) => {
    const label = match[1].trim()
    const pluginSlug = match[2]
    if (!label || pluginSlug.includes("..")) return []
    return [
      {
        label,
        pluginSlug,
        start: match.index,
        end: match.index + match[0].length,
      },
    ]
  })
}
