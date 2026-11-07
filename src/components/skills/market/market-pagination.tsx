"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { useMarketAutoSize } from "./use-market-auto-size"
import {
  MARKET_PAGE_SIZES,
  marketPageCount,
  marketPageNumbers,
} from "@/lib/market-pagination"

export interface MarketPaginationProps {
  page: number
  pageSize: number
  total: number | null
  itemCount?: number
  hasMore?: boolean
  loading: boolean
  onPageChange: (page: number) => void
  onPageSizeChange: (size: number) => void
  automaticPageSize?: boolean
  onAutoPageSizeChange?: (size: number) => void
}

export function MarketPagination(props: MarketPaginationProps) {
  const footer = useMarketAutoSize({
    automatic: props.automaticPageSize,
    size: props.pageSize,
    onChange: props.onAutoPageSizeChange,
  })
  const t = useTranslations("MarketPagination")
  const pages =
    props.total == null
      ? props.page + (props.hasMore ? 1 : 0)
      : marketPageCount(props.total, props.pageSize)
  const start = props.total ? (props.page - 1) * props.pageSize + 1 : 0
  const catalog = useTranslations("CapabilityMarket.catalog")
  return (
    <footer
      ref={footer}
      data-market-pagination
      className="mt-5 flex flex-wrap items-center gap-4 border-t bg-background pt-4 text-[13px]"
    >
      <p
        className="mr-auto text-muted-foreground tabular-nums max-sm:basis-full"
        aria-live="polite"
      >
        {props.loading
          ? t("loading")
          : props.total == null
            ? catalog("unknownPage", {
                page: props.page,
                count: props.itemCount ?? 0,
              })
            : t("range", {
                start: Math.min(start, props.total),
                end: Math.min(props.page * props.pageSize, props.total),
                total: props.total,
              })}
      </p>
      <label className="flex items-center gap-2 text-muted-foreground">
        {t("pageSize")}
        <select
          className="h-8 rounded-md border bg-background px-2 text-foreground focus-visible:outline focus-visible:outline-ring max-sm:min-h-11"
          value={props.automaticPageSize ? 0 : props.pageSize}
          disabled={props.loading}
          onChange={(event) =>
            props.onPageSizeChange(Number(event.target.value))
          }
        >
          {props.onAutoPageSizeChange && (
            <option value={0}>
              {t("automatic", { count: props.pageSize })}
            </option>
          )}
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
        size="sm"
        variant="ghost"
        aria-label={t("previous")}
        title={t("previous")}
        disabled={props.loading || props.page <= 1}
        onClick={() => props.onPageChange(props.page - 1)}
      >
        {t("previous")}
      </Button>
      {props.total != null && props.total > 0 && <NumberedPages {...props} />}
      <Button
        size="sm"
        variant="ghost"
        aria-label={t("next")}
        title={t("next")}
        disabled={props.loading || props.page >= props.pages}
        onClick={() => props.onPageChange(props.page + 1)}
      >
        {t("next")}
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
            page === props.page &&
              "bg-foreground font-semibold text-background hover:bg-foreground/90 hover:text-background"
          )}
          onClick={() => props.onPageChange(page)}
        >
          {page}
        </Button>
      )
  )
}
