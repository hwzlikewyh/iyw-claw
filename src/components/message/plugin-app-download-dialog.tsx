import { useEffect, useRef } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { usePluginAppDownload } from "./use-plugin-app-download"

export function PluginAppDownloadDialog(
  props: ReturnType<typeof usePluginAppDownload>
) {
  const t = useTranslations("PluginDownloads")
  const { pending } = props
  const dialog = useDownloadDialog(pending)
  if (!pending) return null
  return (
    <dialog
      ref={dialog}
      onCancel={(event) => {
        event.preventDefault()
        event.stopPropagation()
        if (!props.busy) props.cancel()
      }}
      className="fixed inset-0 m-auto w-[calc(100%-2rem)] max-w-md rounded-xl border bg-popover p-5 text-popover-foreground shadow-lg backdrop:bg-black/50"
    >
      <h2 className="text-base font-semibold">{t("title")}</h2>
      <p className="mt-3 break-all text-sm">{pending.file.name}</p>
      <p className="mt-1 text-xs text-muted-foreground">
        {t("size", { bytes: pending.file.size })}
      </p>
      <p className="mt-3 text-sm text-muted-foreground">{t("destination")}</p>
      {props.error && (
        <p role="alert" className="mt-3 text-sm text-destructive">
          {props.error}
        </p>
      )}
      <div className="mt-5 flex justify-end gap-2">
        <Button
          variant="outline"
          size="sm"
          disabled={props.busy}
          onClick={props.cancel}
        >
          {t("cancel")}
        </Button>
        <Button
          size="sm"
          disabled={props.busy}
          onClick={() => void props.confirm()}
        >
          {t(props.busy ? "saving" : "save")}
        </Button>
      </div>
    </dialog>
  )
}

function useDownloadDialog(
  pending: ReturnType<typeof usePluginAppDownload>["pending"]
) {
  const dialog = useRef<HTMLDialogElement>(null)
  useEffect(() => {
    const root = dialog.current
    if (!root || !pending) return
    root.showModal()
    return () => {
      if (root.open) root.close()
    }
  }, [pending])
  return dialog
}
