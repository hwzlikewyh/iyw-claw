import type { SessionConfigOptionInfo } from "@/lib/types"

export interface SessionConfigSyncCommand {
  configId: string
  valueId: string
}

function optionValues(option: SessionConfigOptionInfo): Set<string> {
  return new Set(option.kind.options.map(({ value }) => value))
}

function findLiveOption(
  canonicalOption: SessionConfigOptionInfo,
  liveOptions: SessionConfigOptionInfo[]
): SessionConfigOptionInfo | null {
  const direct = liveOptions.find(({ id }) => id === canonicalOption.id)
  if (direct) return direct
  if (!canonicalOption.category) return null

  const semantic = liveOptions.filter(
    ({ category }) => category === canonicalOption.category
  )
  return semantic.length === 1 ? semantic[0] : null
}

function resolveCompatibleValue(
  canonicalOption: SessionConfigOptionInfo,
  liveOption: SessionConfigOptionInfo,
  desiredValue: string
): string | null {
  const values = optionValues(liveOption)
  if (values.has(desiredValue)) return desiredValue

  const isThoughtLevel =
    canonicalOption.category === "thought_level" ||
    liveOption.category === "thought_level"
  const isBinarySwitch =
    values.size === 2 && values.has("off") && values.has("on")
  if (!isThoughtLevel || !isBinarySwitch) return null
  return desiredValue === "off" ? "off" : "on"
}

export function resolveSessionConfigTarget(
  canonicalOption: SessionConfigOptionInfo,
  desiredValue: string,
  liveOptions: SessionConfigOptionInfo[]
): SessionConfigSyncCommand | null {
  const liveOption = findLiveOption(canonicalOption, liveOptions)
  if (!liveOption) return null
  const valueId = resolveCompatibleValue(
    canonicalOption,
    liveOption,
    desiredValue
  )
  return valueId ? { configId: liveOption.id, valueId } : null
}

function isModelOption(option: SessionConfigOptionInfo): boolean {
  return option.id === "model" || option.category === "model"
}

function isCurrentValue(
  command: SessionConfigSyncCommand,
  liveOptions: SessionConfigOptionInfo[]
): boolean {
  return liveOptions.some(
    (option) =>
      option.id === command.configId &&
      option.kind.current_value === command.valueId
  )
}

export function planSessionConfigSync(
  canonicalOptions: SessionConfigOptionInfo[],
  liveOptions: SessionConfigOptionInfo[],
  preferences: Record<string, string>
): SessionConfigSyncCommand[] {
  const pending = canonicalOptions.flatMap((option) => {
    const desiredValue = preferences[option.id]
    if (
      !desiredValue ||
      !option.kind.options.some(({ value }) => value === desiredValue)
    ) {
      return []
    }
    const command = resolveSessionConfigTarget(
      option,
      desiredValue,
      liveOptions
    )
    return command ? [{ option, command }] : []
  })

  const model = pending.find(({ option }) => isModelOption(option))
  if (model && !isCurrentValue(model.command, liveOptions)) {
    return [model.command]
  }

  return pending
    .filter(
      ({ option, command }) =>
        !isModelOption(option) && !isCurrentValue(command, liveOptions)
    )
    .map(({ command }) => command)
}
