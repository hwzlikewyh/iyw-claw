export function formatTokenCount(n: number): string {
  if (n >= 1_000_000) {
    return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`
  }
  if (n >= 1_000) {
    return `${(n / 1_000).toFixed(1).replace(/\.0$/, "")}K`
  }
  return n.toLocaleString()
}

const TOKENS_PER_K = 1_000

export function formatTokenThousands(n: number, locale?: string): string {
  return `${(n / TOKENS_PER_K).toLocaleString(locale, {
    maximumFractionDigits: 3,
  })}K`
}
