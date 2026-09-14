"use client"

import { useState } from "react"
import { ExternalLink, Plug, RefreshCw, Trash2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { openUrl } from "@/lib/platform"
import { toErrorMessage } from "@/lib/app-error"
import { toast } from "sonner"
import { useOpencliSetup, type OpencliSetup } from "./use-opencli-setup"
import { OpencliExtensionDialog } from "./opencli-extension-dialog"
import { OpencliConnectionStatus } from "./opencli-connection-status"

const STORE_URL =
  "https://chromewebstore.google.com/detail/opencli/ildkmabpimmkaediidaifkhjpohdnifk"

export function OpencliBrowserSettings() {
  const t = useTranslations("BrowserSetup")
  const setup = useOpencliSetup()
  const [open, setOpen] = useState(false)
  return (
    <div className="space-y-3 px-4 py-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="text-sm font-medium">{t("externalBrowser")}</div>
          <OpencliConnectionStatus setup={setup} />
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button
            size="sm"
            disabled={!!setup.action}
            onClick={() => {
              setOpen(true)
              void setup.prepare()
            }}
          >
            <Plug className="size-3.5" />
            {t("connect")}
          </Button>
          <Button
            variant="outline"
            size="icon"
            className="size-8"
            title={t("check")}
            aria-label={t("check")}
            disabled={!!setup.action || !setup.tool?.installed}
            onClick={() => void setup.check()}
          >
            <RefreshCw className="size-3.5" />
          </Button>
        </div>
      </div>
      {setup.error && (
        <p role="alert" className="break-words text-xs text-destructive">
          {setup.error}
        </p>
      )}
      <OpencliAdvanced setup={setup} />
      <OpencliExtensionDialog
        open={open}
        onOpenChange={setOpen}
        setup={setup}
      />
    </div>
  )
}

function OpencliAdvanced({ setup }: { setup: OpencliSetup }) {
  const t = useTranslations("BrowserSetup")
  return (
    <details className="text-xs text-muted-foreground">
      <summary className="w-fit cursor-pointer py-1">{t("advanced")}</summary>
      <div className="space-y-2 pt-2">
        <OpencliRuntimeDetails setup={setup} />
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={!!setup.action}
            onClick={() => void setup.repair()}
          >
            <RefreshCw className="size-3.5" />
            {t("repair")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              void openUrl(STORE_URL).catch((error) =>
                toast.error(toErrorMessage(error))
              )
            }
          >
            <ExternalLink className="size-3.5" />
            {t("store")}
          </Button>
          {setup.tool?.installed && (
            <Button
              variant="outline"
              size="icon"
              className="size-8 text-destructive"
              title={t("uninstall")}
              aria-label={t("uninstall")}
              disabled={!!setup.action}
              onClick={() => void setup.uninstall()}
            >
              <Trash2 className="size-3.5" />
            </Button>
          )}
        </div>
      </div>
    </details>
  )
}

function OpencliRuntimeDetails({ setup }: { setup: OpencliSetup }) {
  const t = useTranslations("BrowserSetup")
  return (
    <>
      <p>
        {t("version", {
          version: setup.tool?.version ?? setup.tool?.expectedVersion ?? "-",
        })}
      </p>
      {setup.tool?.path && (
        <code className="block break-all">{setup.tool.path}</code>
      )}
      {(setup.doctor?.message || setup.tool?.runtimeError) && (
        <pre className="max-h-32 overflow-auto whitespace-pre-wrap break-words">
          {setup.doctor?.message || setup.tool?.runtimeError}
        </pre>
      )}
    </>
  )
}
