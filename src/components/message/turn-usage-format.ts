const DECIMAL_SCALE = 18
const HUNDRED = BigInt(100)
const ROUNDING_UNIT = BigInt(10) ** BigInt(DECIMAL_SCALE - 2)

// 只格式化展示，不把后端消费记录转为浮点数或重新计算扣费。
export function formatTurnPoints(
  value: string | number,
  locale: string
): string {
  if (typeof value === "number") {
    return value.toLocaleString(locale, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    })
  }
  const [whole, fraction = ""] = value.split(".")
  const amount =
    BigInt(whole) * BigInt(10) ** BigInt(DECIMAL_SCALE) +
    BigInt(fraction.padEnd(DECIMAL_SCALE, "0"))
  const rounded = (amount + ROUNDING_UNIT / BigInt(2)) / ROUNDING_UNIT
  const separator =
    new Intl.NumberFormat(locale)
      .formatToParts(1.1)
      .find((part) => part.type === "decimal")?.value ?? "."
  return (
    (rounded / HUNDRED).toLocaleString(locale) +
    separator +
    (rounded % HUNDRED).toString().padStart(2, "0")
  )
}
