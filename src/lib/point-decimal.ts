import type { ConfirmedConsumption } from "./types"

const SCALE = 18
const FACTOR = BigInt("1000000000000000000")
const ZERO = BigInt(0)

function units(value: string): bigint | null {
  if (!/^\d+(?:\.\d{1,18})?$/.test(value)) return null
  const [whole, fraction = ""] = value.split(".")
  return BigInt(whole) * FACTOR + BigInt(fraction.padEnd(SCALE, "0"))
}

export function formatPointDecimal(value: string, locale?: string): string {
  const amount = units(value)
  if (amount === null) return "—"
  const hundredth = FACTOR / BigInt(100)
  const separator =
    new Intl.NumberFormat(locale)
      .formatToParts(1.1)
      .find((part) => part.type === "decimal")?.value ?? "."
  if (amount > ZERO && amount < hundredth) return `<0${separator}01`
  const rounded = (amount + hundredth / BigInt(2)) / hundredth
  const whole = rounded / BigInt(100)
  const fraction = (rounded % BigInt(100))
    .toString()
    .padStart(2, "0")
    .replace(/0+$/, "")
  return whole.toLocaleString(locale) + (fraction ? separator + fraction : "")
}

export function mergeConfirmedPoints(
  left?: ConfirmedConsumption | null,
  right?: ConfirmedConsumption | null
): ConfirmedConsumption | null {
  if (!left && !right) return null
  // 同一原生轮次的费用只挂在末尾卡片；其它展示卡片不独立计费。
  if (!left) return right ?? null
  if (!right) return left
  const a = left?.amount == null ? null : units(left.amount)
  const b = right?.amount == null ? null : units(right.amount)
  const hasAmount = a !== null || b !== null
  const total = (a ?? ZERO) + (b ?? ZERO)
  const whole = (total / FACTOR).toString()
  const fraction = (total % FACTOR)
    .toString()
    .padStart(SCALE, "0")
    .replace(/0+$/, "")
  return {
    amount: !hasAmount ? null : whole + (fraction ? "." + fraction : ""),
    state: hasAmount
      ? left.state === "confirmed" &&
        right.state === "confirmed" &&
        a !== null &&
        b !== null
        ? "confirmed"
        : "partial"
      : left.state === "pending" || right.state === "pending"
        ? "pending"
        : "unavailable",
  }
}
