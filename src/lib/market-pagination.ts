export const MARKET_PAGE_SIZES = [6, 12, 24] as const
export const DEFAULT_MARKET_PAGE_SIZE = MARKET_PAGE_SIZES[0]

export function marketPageSize(value: unknown): number {
  const size = Number(value)
  return Number.isSafeInteger(size) && size > 0
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

export function persistMarketPage(
  page: number,
  size: number,
  automatic = false
): void {
  if (typeof window === "undefined") return
  const url = new URL(window.location.href)
  if (page === 1) url.searchParams.delete("page")
  else url.searchParams.set("page", String(page))
  if (automatic) url.searchParams.delete("pageSize")
  else url.searchParams.set("pageSize", String(size))
  window.history.replaceState(
    null,
    "",
    `${url.pathname}${url.search}${url.hash}`
  )
}

export function fitMarketPage(input: {
  availableHeight: number
  cardHeight: number
  rowGap: number
  columns: number
}): number {
  const { availableHeight, cardHeight, rowGap, columns } = input
  if (
    ![availableHeight, cardHeight, rowGap, columns].every(Number.isFinite) ||
    cardHeight <= 0 ||
    columns < 1
  )
    return DEFAULT_MARKET_PAGE_SIZE
  const rows = Math.max(
    1,
    Math.floor((availableHeight + rowGap) / (cardHeight + rowGap))
  )
  return Math.max(1, Math.floor(columns)) * rows
}
