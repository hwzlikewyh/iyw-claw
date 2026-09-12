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
          className="h-8 w-8 shrink-0"
          disabled={disabled || pending}
          aria-label={t("forkSession")}
          aria-busy={pending}
          onClick={onFork}
        >
          {pending ? (
            <Loader2 aria-hidden="true" className="size-4 animate-spin" />
          ) : (
            <GitFork aria-hidden="true" className="size-4" />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{t("forkSession")}</TooltipContent>
    </Tooltip>
  )
}
