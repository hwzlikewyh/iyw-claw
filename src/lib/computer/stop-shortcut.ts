export interface StopShortcutParts {
  control: boolean
  alt: boolean
  shift: boolean
  command: boolean
  code: string
}
export type StopShortcutProblem =
  | "unsupportedKey"
  | "weakModifiers"
  | "metaOffMac"
const PUNCTUATION = new Map<string, string>([
  ["Minus", "-"],
  ["Equal", "="],
  ["BracketLeft", "["],
  ["BracketRight", "]"],
  ["Backslash", "\\"],
  ["Semicolon", ";"],
  ["Quote", "'"],
  ["Backquote", "`"],
  ["Comma", ","],
  ["Period", "."],
  ["Slash", "/"],
])
const MODIFIER_NAMES = new Map<string, "control" | "alt" | "shift" | "command">(
  [
    ["Control", "control"],
    ["Alt", "alt"],
    ["Shift", "shift"],
    ["Command", "command"],
  ]
)
const MODIFIER_CODES = new Set([
  "ShiftLeft",
  "ShiftRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
  "CapsLock",
  "Fn",
])
export function isAllowedStopKey(code: string): boolean {
  return (
    /^Key[A-Z]$/.test(code) ||
    /^Digit[0-9]$/.test(code) ||
    /^F([1-9]|1[0-2])$/.test(code) ||
    code === "Escape" ||
    PUNCTUATION.has(code)
  )
}
export function parseStopShortcut(spelling: string): StopShortcutParts | null {
  if (!spelling) return null
  const parts = spelling.split("+")
  const code = parts.pop()
  if (!code) return null
  const out: StopShortcutParts = {
    control: false,
    alt: false,
    shift: false,
    command: false,
    code,
  }
  for (const part of parts) {
    const key = MODIFIER_NAMES.get(part)
    if (!key || out[key]) return null
    out[key] = true
  }
  return out
}
export function spellStopShortcut(parts: StopShortcutParts): string {
  return [
    parts.control && "Control",
    parts.alt && "Alt",
    parts.shift && "Shift",
    parts.command && "Command",
    parts.code,
  ]
    .filter(Boolean)
    .join("+")
}
export function stopShortcutProblem(
  parts: StopShortcutParts,
  isMac: boolean
): StopShortcutProblem | null {
  if (!isAllowedStopKey(parts.code)) return "unsupportedKey"
  if (parts.command && !isMac) return "metaOffMac"
  const held = [parts.control, parts.alt, parts.shift, parts.command].filter(
    Boolean
  ).length
  if (held < 2 || !(parts.control || parts.command)) return "weakModifiers"
  return null
}
export function stopShortcutFromEvent(
  event: Pick<
    KeyboardEvent,
    "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
  >
): StopShortcutParts | null {
  if (!event.code || MODIFIER_CODES.has(event.code)) return null
  return {
    control: event.ctrlKey,
    alt: event.altKey,
    shift: event.shiftKey,
    command: event.metaKey,
    code: event.code,
  }
}
function keyLabel(code: string): string {
  const letter = /^Key([A-Z])$/.exec(code)
  if (letter) return letter[1]
  const digit = /^Digit([0-9])$/.exec(code)
  if (digit) return digit[1]
  if (code === "Escape") return "Esc"
  return PUNCTUATION.get(code) ?? code
}
export function stopShortcutLabel(spelling: string, isMac: boolean): string {
  const parts = parseStopShortcut(spelling)
  if (!parts) return spelling
  const key = keyLabel(parts.code)
  if (isMac) {
    return [
      parts.control && "⌃",
      parts.alt && "⌥",
      parts.shift && "⇧",
      parts.command && "⌘",
      key,
    ]
      .filter(Boolean)
      .join("")
  }
  return [
    parts.control && "Ctrl",
    parts.alt && "Alt",
    parts.shift && "Shift",
    parts.command && "Win",
    key,
  ]
    .filter(Boolean)
    .join("+")
}
export function defaultStopShortcut(isMac: boolean): string {
  return isMac ? "Control+Command+Escape" : "Control+Alt+Escape"
}
