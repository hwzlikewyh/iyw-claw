"use client"

import { useState } from "react"
import { ExternalLink, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { openBrowserExtensionSettings } from "@/lib/browser-setup-api"
import { toErrorMessage } from "@/lib/app-error"
import { OpencliConnectionStatus } from "./opencli-connection-status"
import { OpencliExtensionDirectory } from "./opencli-extension-directory"
import { SegmentedControl } from "./settings-ui"
import { useOpencliPolling, type OpencliSetup } from "./use-opencli-setup"

interface DialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  setup: OpencliSetup
}

export function OpencliExtensionDialog(props: DialogProps) {
  const t = useTranslations("BrowserSetup")
  const { open, onOpenChange, setup } = props
  const checking = useOpencliPolling(
    open &&
      !!setup.directory &&
      setup.tool?.status === "installed" &&
      setup.action !== "installing" &&
      setup.action !== "preparing",
    setup.check
  )
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("connect")}</DialogTitle>
          <DialogDescription>{t("extensionTitle")}</DialogDescription>
        </DialogHeader>
        <OpencliConnectionStatus setup={setup} />
        {setup.directory && (
          <OpencliExtensionSteps directory={setup.directory} />
        )}
        {checking && !setup.doctor?.ok && (
          <p className="text-xs text-muted-foreground">{t("waiting")}</p>
        )}
        {setup.error && (
          <p role="alert" className="break-words text-xs text-destructive">
            {setup.error}
          </p>
        )}
        {setup.doctor && !setup.doctor.ok && (
          <details className="text-xs text-muted-foreground">
            <summary className="cursor-pointer">{t("advanced")}</summary>
            <pre className="mt-2 max-h-28 overflow-auto whitespace-pre-wrap break-words">
              {setup.doctor.message}
            </pre>
          </details>
        )}
        <OpencliExtensionFooter {...props} />
      </DialogContent>
    </Dialog>
  )
}

function OpencliExtensionSteps({ directory }: { directory: string }) {
  const t = useTranslations("BrowserSetup")
  const [browser, setBrowser] = useState<"chrome" | "edge">("chrome")
  const [error, setError] = useState("")
  const openSettings = async () => {
    setError("")
    try {
      await openBrowserExtensionSettings(browser)
    } catch (cause) {
      setError(toErrorMessage(cause))
    }
  }
  return (
    <div className="space-y-4">
      <SegmentedControl
        value={browser}
        onChange={setBrowser}
        options={[
          { value: "chrome", label: "Chrome" },
          { value: "edge", label: "Edge" },
        ]}
      />
      <ol className="list-decimal space-y-2 pl-5 text-sm">
        <li>{t("stepOpen")}</li>
        <li>{t("stepDeveloper")}</li>
        <li>{t("stepLoad")}</li>
      </ol>
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" onClick={() => void openSettings()}>
          <ExternalLink className="size-3.5" />
          {t("openSettings")}
        </Button>
        <code className="select-all break-all text-xs text-muted-foreground">
          {browser}://extensions
        </code>
      </div>
      {error && (
        <p role="alert" className="break-words text-xs text-destructive">
          {error}
        </p>
      )}
      <OpencliExtensionDirectory directory={directory} />
    </div>
  )
}

function OpencliExtensionFooter({ onOpenChange, setup }: DialogProps) {
  const t = useTranslations("BrowserSetup")
  const ready = !!setup.directory && setup.tool?.status === "installed"
  return (
    <div className="flex justify-end gap-2">
      <Button
        variant="outline"
        size="sm"
        disabled={!!setup.action}
        onClick={() => void (ready ? setup.check() : setup.prepare())}
      >
        <RefreshCw className="size-3.5" />
        {ready ? t("check") : t("retry")}
      </Button>
      <Button size="sm" onClick={() => onOpenChange(false)}>
        {setup.doctor?.ok ? t("done") : t("close")}
      </Button>
    </div>
  )
}
