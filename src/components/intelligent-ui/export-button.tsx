"use client"

import { useRef, useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { toErrorMessage } from "@/lib/app-error"
import { isDesktop } from "@/lib/platform"

export function UiExportButton({
  exportFile,
  variant = "ghost",
}: {
  exportFile: () => Promise<boolean>
  variant?: "ghost" | "outline"
}) {
  const t = useTranslations("IntelligentUI")
  const { saving, run } = useExportAction(exportFile)
  return (
    <Button
      type="button"
      size="sm"
      variant={variant}
      disabled={saving}
      aria-busy={saving}
      onClick={() => void run()}
    >
      {saving && <Loader2 className="size-3.5 animate-spin" />}
      {t("export")}
    </Button>
  )
}

function useExportAction(exportFile: () => Promise<boolean>) {
  const t = useTranslations("Folder.taskArtifacts")
  const ui = useTranslations("IntelligentUI")
  const busy = useRef(false)
  const [saving, setSaving] = useState(false)
  async function run() {
    if (busy.current) return
    busy.current = true
    setSaving(true)
    try {
      if (await exportFile())
        toast.success(isDesktop() ? t("fileSaved") : ui("downloadStarted"))
    } catch (error) {
      console.error("[intelligent-ui] export failed", {
        error: toErrorMessage(error),
      })
      toast.error(t("downloadFailed"), { description: toErrorMessage(error) })
    } finally {
      busy.current = false
      setSaving(false)
    }
  }
  return { saving, run }
}
