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
  if ((left.account_scope ?? null) !== (right.account_scope ?? null))
    return { ...right, state: right.amount == null ? right.state : "partial" }
  if (left.records !== undefined || right.records !== undefined)
    return mergeStages(left, right)
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

function decimal(amount: bigint): string {
  const whole = (amount / FACTOR).toString()
  const fraction = (amount % FACTOR)
    .toString()
    .padStart(SCALE, "0")
    .replace(/0+$/, "")
  return whole + (fraction ? "." + fraction : "")
}

function mergeStages(
  left: ConfirmedConsumption,
  right: ConfirmedConsumption
): ConfirmedConsumption {
  const stages = new Map<
    string,
    NonNullable<ConfirmedConsumption["records"]>[number]
  >()
  for (const row of [...(left.records ?? []), ...(right.records ?? [])])
    stages.set(JSON.stringify([row.request_id, row.stage_key]), row)
  let total = ZERO
  let charged = ZERO
  let available = false
  let complete =
    left.state === "confirmed" &&
    right.state === "confirmed" &&
    left.records !== undefined &&
    right.records !== undefined
  let chargedComplete = complete
  const settlement_statuses: Record<string, number> = {}
  for (const row of stages.values()) {
    const actual = row.actual_points == null ? null : units(row.actual_points)
    const debit = row.charged_points == null ? null : units(row.charged_points)
    available ||= actual !== null
    complete &&= actual !== null
    chargedComplete &&= debit !== null
    total += actual ?? ZERO
    charged += debit ?? ZERO
    settlement_statuses[row.settlement_status] =
      (settlement_statuses[row.settlement_status] ?? 0) + 1
  }
  const amount = available ? decimal(total) : null
  return {
    amount,
    account_scope: right.account_scope,
    state:
      amount !== null
        ? complete
          ? "confirmed"
          : "partial"
        : left.state === "pending" || right.state === "pending"
          ? "pending"
          : "unavailable",
    charged_points:
      chargedComplete && complete && stages.size > 0 ? decimal(charged) : null,
    settlement_statuses,
    records: [...stages.values()],
  }
}
