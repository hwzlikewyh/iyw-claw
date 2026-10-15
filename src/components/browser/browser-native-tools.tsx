"use client"

import { useEffect, useState } from "react"
import { ArrowDown, ArrowUp, Bug, Search, Square } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { browserApi } from "@/lib/browser-api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { toErrorMessage } from "@/lib/app-error"
import { getShellTransport } from "@/lib/transport"

export function BrowserNativeTools({ tabId }: { tabId: string }) {
  const t = useTranslations("GeneralSettings")
  const [query, setQuery] = useState("")
  const [open, setOpen] = useState(false)
  useEffect(() => {
    let disposed = false
    let stop: (() => void) | undefined
    void getShellTransport()
      .subscribe<string>("browser://native-find", (id) => {
        if (id === tabId) setOpen(true)
      })
      .then((unsubscribe) => {
        if (disposed) unsubscribe()
        else stop = unsubscribe
      })
      .catch((error) =>
        console.warn("[Browser] find shortcut subscription failed", error)
      )
    return () => {
      disposed = true
      stop?.()
    }
  }, [tabId])
  const run = (action: string, values: Record<string, unknown> = {}) => {
    void browserApi
      .nativeAction(tabId, action, values)
      .catch((error) => toast.error(toErrorMessage(error)))
  }
  return (
    <>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="size-7"
            title={t("browserFind")}
            aria-label={t("browserFind")}
          >
            <Search className="size-3.5" />
          </Button>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-72 p-2">
          <div className="flex items-center gap-1">
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("browserFindPlaceholder")}
              aria-label={t("browserFind")}
              className="h-7 min-w-0 flex-1 text-xs"
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault()
                  run("find", { query, backwards: event.shiftKey })
                }
              }}
            />
            <Button
              variant="ghost"
              size="icon"
              className="size-7"
              aria-label={t("browserFindPrevious")}
              onClick={() => run("find", { query, backwards: true })}
            >
              <ArrowUp className="size-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="size-7"
              aria-label={t("browserFindNext")}
              onClick={() => run("find", { query, backwards: false })}
            >
              <ArrowDown className="size-3.5" />
            </Button>
          </div>
          <div className="mt-2 flex justify-end gap-1 border-t pt-2">
            <Button
              variant="ghost"
              size="icon"
              className="size-7"
              aria-label={t("browserDevtools")}
              title={t("browserDevtools")}
              onClick={() => run("devtools")}
            >
              <Bug className="size-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="size-7"
              aria-label={t("browserStopLoading")}
              title={t("browserStopLoading")}
              onClick={() => run("stop")}
            >
              <Square className="size-3.5" />
            </Button>
          </div>
        </PopoverContent>
      </Popover>
    </>
  )
}
