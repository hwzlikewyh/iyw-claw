"use client"

import { useTranslations } from "next-intl"
import { ChevronLeft, ChevronRight } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import {
  MARKET_PAGE_SIZES,
  marketPageCount,
  marketPageNumbers,
} from "@/lib/market-pagination"

export interface MarketPaginationProps {
  page: number
  pageSize: number
  total: number
  loading: boolean
  onPageChange: (page: number) => void
  onPageSizeChange: (size: number) => void
}

export function MarketPagination(props: MarketPaginationProps) {
  const t = useTranslations("MarketPagination")
  const pages = marketPageCount(props.total, props.pageSize)
  const start = props.total ? (props.page - 1) * props.pageSize + 1 : 0
  return (
    <footer className="flex shrink-0 flex-wrap items-center justify-between gap-x-4 gap-y-2 border-t bg-background px-4 py-3 text-xs sm:px-5">
      <p className="text-muted-foreground tabular-nums" aria-live="polite">
        {props.loading
          ? t("loading")
          : t("range", {
              start: Math.min(start, props.total),
              end: Math.min(props.page * props.pageSize, props.total),
              total: props.total,
            })}
      </p>
      <label className="flex items-center gap-2 text-muted-foreground">
        {t("pageSize")}
        <select
          className="h-8 rounded-md border bg-background px-2 text-foreground focus-visible:outline focus-visible:outline-ring"
          value={props.pageSize}
          disabled={props.loading}
          onChange={(event) =>
            props.onPageSizeChange(Number(event.target.value))
          }
        >
          {MARKET_PAGE_SIZES.map((size) => (
            <option key={size} value={size}>
              {size}
            </option>
          ))}
        </select>
      </label>
      <PageButtons {...props} pages={pages} />
    </footer>
  )
}

function PageButtons(props: MarketPaginationProps & { pages: number }) {
  const t = useTranslations("MarketPagination")
  return (
    <nav aria-label={t("label")} className="flex items-center gap-1">
      <Button
        size="icon-sm"
        variant="ghost"
        aria-label={t("previous")}
        title={t("previous")}
        disabled={props.loading || props.page <= 1}
        onClick={() => props.onPageChange(props.page - 1)}
      >
        <ChevronLeft className="size-4" />
      </Button>
      <NumberedPages {...props} />
      <Button
        size="icon-sm"
        variant="ghost"
        aria-label={t("next")}
        title={t("next")}
        disabled={props.loading || props.page >= props.pages}
        onClick={() => props.onPageChange(props.page + 1)}
      >
        <ChevronRight className="size-4" />
      </Button>
    </nav>
  )
}

function NumberedPages(props: MarketPaginationProps & { pages: number }) {
  const t = useTranslations("MarketPagination")
  return marketPageNumbers(props.page, Math.max(props.page, props.pages)).map(
    (page) =>
      typeof page === "string" ? (
        <span
          key={page}
          className="px-1 text-muted-foreground"
          aria-hidden="true"
        >
          …
        </span>
      ) : (
        <Button
          key={page}
          size="icon-sm"
          variant="ghost"
          disabled={props.loading}
          aria-label={t("page", { page })}
          aria-current={page === props.page ? "page" : undefined}
          className={cn(
            "max-w-16 truncate px-1 tabular-nums",
            page === props.page && "bg-muted font-semibold"
          )}
          onClick={() => props.onPageChange(page)}
        >
          {page}
        </Button>
      )
  )
}
