"use client"

import { useCallback, useState, type ComponentType } from "react"
import { Globe, Image, Link, ScanEye, QrCode, X } from "lucide-react"
import { QRCodeSVG } from "qrcode.react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { openUrl } from "@/lib/platform"
import { copyTextToClipboard } from "@/lib/utils"
import { toErrorMessage } from "@/lib/app-error"
import type { useShareComposer } from "./use-share-composer"
import { ShareImageExport } from "./share-image-export"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog"

type Composer = ReturnType<typeof useShareComposer>

export function ShareSelectionToolbar({
  state,
  close,
}: {
  state: Composer
  close: () => void
}) {
  const actions = useShareActions(state)
  const { action, done, busy } = actions
  return (
    <div className="flex flex-wrap items-center gap-3 border-t pt-4 pb-1">
      <label className="mr-auto flex items-center gap-2 text-xs text-muted-foreground">
        <Checkbox
          checked={state.all ? true : state.ids.size ? "indeterminate" : false}
          onCheckedChange={state.toggleAll}
          disabled={busy || !!state.result || !state.source}
        />
        全选 <span>已选 {state.ids.size} 条</span>
      </label>
      <ToolbarActions state={state} actions={actions} />
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label="退出分享"
        title="退出分享"
        disabled={busy}
        onClick={close}
      >
        <X className="size-4" />
      </Button>
      {state.error && (
        <p role="alert" className="w-full text-xs text-destructive">
          {state.error}
        </p>
      )}
      {state.status && (
        <p role="status" className="w-full text-xs text-muted-foreground">
          {state.status}
        </p>
      )}
      {action === "image" && <ShareImageExport state={state} done={done} />}
      <ShareQRCode url={actions.qrURL} close={() => actions.setQrURL("")} />
    </div>
  )
}

function useShareActions(state: Composer) {
  const [action, setAction] = useState("")
  const [qrURL, setQrURL] = useState("")
  const done = useCallback(() => setAction(""), [])
  const busy = !!state.status || !!action
  const disabled =
    busy || !state.snapshot?.messages.length || !state.title.trim()
  const run = async (kind: string) => {
    if (disabled) return
    setAction(kind)
    try {
      if (kind === "image") return
      const url = await state.publish()
      if (!url) return
      if (kind === "browser") await openUrl(url)
      else if (kind === "share") setQrURL(url)
      else if (await copyTextToClipboard(url)) toast.success("分享链接已复制")
      else throw new Error("复制失败，请重试。")
    } catch (error) {
      if (!(error instanceof Error) || error.name !== "AbortError")
        toast.error(toErrorMessage(error))
    } finally {
      if (kind !== "image") setAction("")
    }
  }
  return { action, busy, disabled, run, done, qrURL, setQrURL }
}

function ToolbarActions({
  state,
  actions,
}: {
  state: Composer
  actions: ReturnType<typeof useShareActions>
}) {
  const { action, disabled, run } = actions
  const buttons = [
    { kind: "share", label: "扫码分享", icon: QrCode },
    { kind: "copy", label: "复制链接", icon: Link },
    {
      kind: "image",
      label: action === "image" ? "正在生成…" : "生成长图",
      icon: Image,
    },
    { kind: "browser", label: "浏览器打开", icon: Globe },
  ]
  return (
    <div className="flex flex-1 items-center justify-center gap-2 sm:gap-7">
      {buttons.map((button) => (
        <ShareAction
          key={button.kind}
          label={button.label}
          icon={button.icon}
          disabled={disabled}
          onClick={() => void run(button.kind)}
        />
      ))}
      <ShareAction
        label="预览"
        icon={ScanEye}
        disabled={disabled || !!state.result}
        onClick={() => void state.prepare()}
      />
    </div>
  )
}

function ShareQRCode({ url, close }: { url: string; close: () => void }) {
  return (
    <Dialog
      open={!!url}
      onOpenChange={(open) => {
        if (!open) close()
      }}
    >
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>扫码分享</DialogTitle>
          <DialogDescription>
            使用微信或手机相机扫描，直接打开所选对话。
          </DialogDescription>
        </DialogHeader>
        <div className="mx-auto rounded-xl bg-white p-4">
          {url && <QRCodeSVG value={url} size={220} marginSize={2} />}
        </div>
      </DialogContent>
    </Dialog>
  )
}

function ShareAction({
  label,
  icon: Icon,
  disabled,
  onClick,
}: {
  label: string
  icon: ComponentType<{ className?: string }>
  disabled: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className="flex min-w-14 flex-col items-center gap-2 rounded-md px-1 text-[11px] whitespace-nowrap text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40"
    >
      <span className="flex size-11 items-center justify-center rounded-xl bg-primary/5 text-foreground/80">
        <Icon className="size-5" />
      </span>
      {label}
    </button>
  )
}
