import { ChevronLeft, ChevronRight } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"

interface PaginationProps {
  page: number
  loading: boolean
  hasNext: boolean
  previous: () => void
  next: () => void
}

export function ConversationSearchPagination(props: PaginationProps) {
  const t = useTranslations("Folder.taskArtifacts")
  if (props.page === 1 && !props.hasNext) return null
  return (
    <div className="flex items-center justify-end gap-2 px-2 py-2">
      <Button
        variant="ghost"
        size="icon-sm"
        disabled={props.page === 1 || props.loading}
        onClick={props.previous}
        title={t("paginationPrevious")}
        aria-label={t("paginationPrevious")}
      >
        <ChevronLeft className="size-4" />
      </Button>
      <span className="text-xs tabular-nums text-muted-foreground">
        {props.page}
      </span>
      <Button
        variant="ghost"
        size="icon-sm"
        disabled={!props.hasNext || props.loading}
        onClick={props.next}
        title={t("paginationNext")}
        aria-label={t("paginationNext")}
      >
        <ChevronRight className="size-4" />
      </Button>
    </div>
  )
}
