"use client"

import { GitFork, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"

export function SessionForkButton({
  onFork,
  disabled,
  pending,
}: {
  onFork: () => void
  disabled: boolean
  pending: boolean
}) {
  const t = useTranslations("Folder.conversation")
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="h-6 w-6 shrink-0 rounded-full text-muted-foreground"
          disabled={disabled || pending}
          aria-label={t("forkSession")}
          aria-busy={pending}
          onClick={onFork}
        >
          {pending ? (
            <Loader2 aria-hidden="true" className="size-3.5 animate-spin" />
          ) : (
            <GitFork aria-hidden="true" className="size-3.5" />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{t("forkSession")}</TooltipContent>
    </Tooltip>
  )
}
