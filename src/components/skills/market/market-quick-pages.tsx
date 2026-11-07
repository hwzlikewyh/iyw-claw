import { ChevronLeft, ChevronRight } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { MarketPaginationProps } from "./market-pagination"
import { marketPageCount } from "@/lib/market-pagination"

export function MarketQuickPages(props: MarketPaginationProps) {
  const t = useTranslations("MarketPagination")
  const c = useTranslations("CapabilityMarket.catalog")
  const pages =
    props.total == null ? null : marketPageCount(props.total, props.pageSize)
  return (
    <div className="ml-auto flex items-center gap-[7px] text-xs text-muted-foreground">
      <span role="status">
        {props.loading
          ? t("loading")
          : pages == null
            ? c("unknownPage", {
                page: props.page,
                count: props.itemCount ?? 0,
              })
            : c("quickPages", {
                total: props.total ?? 0,
                page: props.total ? props.page : 0,
                pages: props.total ? pages : 0,
              })}
      </span>
      <Button
        size="icon-sm"
        variant="ghost"
        className="rounded-md max-sm:min-h-11"
        aria-label={t("previous")}
        disabled={props.loading || props.page <= 1}
        onClick={() => props.onPageChange(props.page - 1)}
      >
        <ChevronLeft className="size-4" />
      </Button>
      <Button
        size="icon-sm"
        variant="ghost"
        className="rounded-md max-sm:min-h-11"
        aria-label={t("next")}
        disabled={
          props.loading ||
          (pages == null ? !props.hasMore : props.page >= pages)
        }
        onClick={() => props.onPageChange(props.page + 1)}
      >
        <ChevronRight className="size-4" />
      </Button>
    </div>
  )
}
