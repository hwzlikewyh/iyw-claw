"use client"

import { useCallback, useState, type MouseEvent, type ReactNode } from "react"
import { FolderSearch } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { toErrorMessage } from "@/lib/app-error"
import type { LocalPathPresentation } from "@/lib/local-path-links"
import { isLocalDesktop, openLocalPath, revealItemInDir } from "@/lib/platform"
import { cn } from "@/lib/utils"

type PathAction = "open" | "reveal"

interface LocalPathActionsProps {
  path: string
  children?: ReactNode
  presentation?: LocalPathPresentation
  className?: string
}

function useLocalPathAction(path: string) {
  const t = useTranslations("Folder.chat.linkSafety")
  const [activeAction, setActiveAction] = useState<PathAction | null>(null)

  const runAction = useCallback(
    async (action: PathAction) => {
      if (activeAction) return
      if (!isLocalDesktop()) {
        toast.error(t("errorCannotOpen"), {
          description: t("errorDesktopOnly"),
        })
        return
      }

      setActiveAction(action)
      try {
        if (action === "open") await openLocalPath(path)
        else await revealItemInDir(path)
      } catch (error) {
        toast.error(
          t(action === "open" ? "errorFailedOpen" : "errorFailedReveal"),
          {
            description: toErrorMessage(error),
          }
        )
      } finally {
        setActiveAction(null)
      }
    },
    [activeAction, path, t]
  )

  return {
    activeAction,
    runAction: (event: MouseEvent<HTMLButtonElement>, action: PathAction) => {
      event.preventDefault()
      event.stopPropagation()
      void runAction(action)
    },
  }
}

function RevealLocalPathButton({
  activeAction,
  onAction,
}: {
  activeAction: PathAction | null
  onAction: (event: MouseEvent<HTMLButtonElement>, action: PathAction) => void
}) {
  const t = useTranslations("Folder.chat.linkSafety")
  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            type="button"
            aria-label={t("reveal")}
            aria-busy={activeAction === "reveal"}
            disabled={activeAction !== null}
            onClick={(event) => onAction(event, "reveal")}
            className="ml-1 inline-flex size-5 shrink-0 cursor-pointer items-center justify-center text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-wait disabled:opacity-70"
          >
            <FolderSearch aria-hidden="true" className="size-3.5" />
          </button>
        </TooltipTrigger>
        <TooltipContent>{t("reveal")}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}

export function LocalPathActions({
  path,
  children,
  presentation = "text",
  className,
}: LocalPathActionsProps) {
  const t = useTranslations("Folder.chat.linkSafety")
  const { activeAction, runAction } = useLocalPathAction(path)

  return (
    <span
      className={cn(
        "inline-flex max-w-full items-center align-baseline",
        presentation === "inline-code" &&
          "rounded bg-muted px-1.5 py-0.5 font-mono text-sm",
        className
      )}
    >
      <button
        type="button"
        title={t("openDefault")}
        aria-busy={activeAction === "open"}
        disabled={activeAction !== null}
        onClick={(event) => runAction(event, "open")}
        className="wrap-anywhere min-w-0 cursor-pointer text-left font-medium text-primary underline hover:opacity-80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-wait disabled:opacity-70"
      >
        {children ?? path}
      </button>
      <RevealLocalPathButton activeAction={activeAction} onAction={runAction} />
    </span>
  )
}
