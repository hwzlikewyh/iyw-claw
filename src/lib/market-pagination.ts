export const MARKET_PAGE_SIZES = [6, 12, 24] as const
export const DEFAULT_MARKET_PAGE_SIZE = MARKET_PAGE_SIZES[0]

export function marketPageSize(value: unknown): number {
  const size = Number(value)
  return MARKET_PAGE_SIZES.some((option) => option === size)
    ? size
    : DEFAULT_MARKET_PAGE_SIZE
}

export function marketPage(value: unknown): number {
  const page = Number(value)
  return Number.isSafeInteger(page) &&
    page > 0 &&
    page <=
      Number.MAX_SAFE_INTEGER / MARKET_PAGE_SIZES[MARKET_PAGE_SIZES.length - 1]
    ? page
    : 1
}

export function marketPageCount(total: number, size: number): number {
  return Math.max(1, Math.ceil(Math.max(0, total) / size))
}

export function marketPageNumbers(
  page: number,
  pages: number
): (number | string)[] {
  const selected = [...new Set([1, pages, page - 1, page, page + 1])]
    .filter((value) => value >= 1 && value <= pages)
    .sort((left, right) => left - right)
  return selected.flatMap((value, index) =>
    index > 0 && value - selected[index - 1] > 1
      ? [`gap-${value}`, value]
      : [value]
  )
}

export function persistMarketPage(page: number, size: number): void {
  if (typeof window === "undefined") return
  const url = new URL(window.location.href)
  if (page === 1) url.searchParams.delete("page")
  else url.searchParams.set("page", String(page))
  if (size === DEFAULT_MARKET_PAGE_SIZE) url.searchParams.delete("pageSize")
  else url.searchParams.set("pageSize", String(size))
  window.history.replaceState(
    null,
    "",
    `${url.pathname}${url.search}${url.hash}`
  )
}
