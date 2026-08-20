"use client"

import { useState } from "react"
import { CheckCircle2, Loader2, RefreshCw, ShieldAlert } from "lucide-react"
import { useTranslations } from "next-intl"
import { QRCodeSVG } from "qrcode.react"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { useChatChannelQrcode } from "@/hooks/use-chat-channel-qrcode"
import type { ChatChannelQrViewState } from "@/hooks/use-chat-channel-qrcode"
import type { ChannelType } from "@/lib/types"

interface ChatChannelQrcodeDialogProps {
  open: boolean
  channelId: number
  channelType: Extract<
    ChannelType,
    "weixin" | "wecom_ai_bot" | "dingtalk" | "lark"
  >
  variant?: "feishu" | "lark"
  onOpenChange: (open: boolean) => void
  onAuthSuccess: (channelId: number) => void
}

const CHANNEL_LABEL = {
  weixin: "weixin",
  wecom_ai_bot: "wecomAiBot",
  dingtalk: "dingtalk",
  lark: "lark",
} as const

const STATUS_KEYS = {
  loading: "qr.status.loading",
  waiting: "qr.status.waiting",
  scanned: "qr.status.scanned",
  connecting: "qr.status.connecting",
  connected: "qr.status.connected",
  expired: "qr.status.expired",
  denied: "qr.status.denied",
  cancelled: "qr.status.cancelled",
  error: "qr.status.error",
  verify_code_required: "qr.status.verify_code_required",
  verify_code_blocked: "qr.status.verify_code_blocked",
  already_bound: "qr.status.already_bound",
} as const

export function ChatChannelQrcodeDialog({
  open,
  channelId,
  channelType,
  variant,
  onOpenChange,
  onAuthSuccess,
}: ChatChannelQrcodeDialogProps) {
  const t = useTranslations("ChatChannelSettings")
  const [verifyCode, setVerifyCode] = useState("")
  const { state, restart, submitVerifyCode } = useChatChannelQrcode({
    active: open,
    channelId,
    channelType,
    variant,
    onConnected: () => onAuthSuccess(channelId),
  })

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>
            {t("qr.title", { channel: t(CHANNEL_LABEL[channelType]) })}
          </DialogTitle>
        </DialogHeader>
        <QrcodeDialogBody
          channelType={channelType}
          state={state}
          verifyCode={verifyCode}
          onVerifyCodeChange={setVerifyCode}
          onVerify={() => {
            if (verifyCode.trim()) void submitVerifyCode(verifyCode.trim())
          }}
          onRefresh={() => {
            setVerifyCode("")
            restart()
          }}
        />
      </DialogContent>
    </Dialog>
  )
}

function QrcodeDialogBody({
  channelType,
  state,
  verifyCode,
  onVerifyCodeChange,
  onVerify,
  onRefresh,
}: {
  channelType: ChatChannelQrcodeDialogProps["channelType"]
  state: ChatChannelQrViewState
  verifyCode: string
  onVerifyCodeChange: (value: string) => void
  onVerify: () => void
  onRefresh: () => void
}) {
  const t = useTranslations("ChatChannelSettings")
  const verificationRequired = state.errorCode === "verify_code_required"
  const terminal = ["expired", "denied", "cancelled", "error"].includes(
    state.status
  )
  return (
    <div className="flex flex-col items-center gap-4 py-3">
      <p className="text-center text-sm text-muted-foreground">
        {t(`qr.descriptions.${channelType}`)}
      </p>
      <QrVisual content={state.session?.qrContent} status={state.status} />
      <QrStatus {...state} />
      {verificationRequired && (
        <div className="flex w-full gap-2">
          <Input
            value={verifyCode}
            onChange={(event) => onVerifyCodeChange(event.target.value)}
            placeholder={t("qr.verifyCodePlaceholder")}
            onKeyDown={(event) => event.key === "Enter" && onVerify()}
          />
          <Button size="sm" disabled={!verifyCode.trim()} onClick={onVerify}>
            {t("qr.continue")}
          </Button>
        </div>
      )}
      {terminal && (
        <Button variant="outline" size="sm" onClick={onRefresh}>
          <RefreshCw className="h-3.5 w-3.5" />
          {t("qr.refresh")}
        </Button>
      )}
    </div>
  )
}

function QrVisual({ content, status }: { content?: string; status: string }) {
  const busy = status === "loading" || status === "connecting"
  if (["expired", "denied", "cancelled", "error"].includes(status)) {
    return (
      <div className="flex h-52 w-52 items-center justify-center rounded-md border bg-muted/30">
        <ShieldAlert className="h-10 w-10 text-destructive" />
      </div>
    )
  }
  if (busy || !content) {
    return (
      <div className="flex h-52 w-52 items-center justify-center rounded-md border bg-muted/30">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    )
  }
  if (status === "connected") {
    return (
      <div className="flex h-52 w-52 items-center justify-center rounded-md border bg-muted/30">
        <CheckCircle2 className="h-10 w-10 text-emerald-500" />
      </div>
    )
  }
  return (
    <div className="flex h-52 w-52 items-center justify-center overflow-hidden rounded-md border bg-white p-2">
      {content.startsWith("data:image/") ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img src={content} alt="QR code" className="h-full w-full" />
      ) : (
        <QRCodeSVG value={content} size={192} marginSize={0} />
      )}
    </div>
  )
}

function QrStatus({
  status,
  errorCode,
  error,
}: {
  status: string
  errorCode: string | null
  error: string | null
}) {
  const t = useTranslations("ChatChannelSettings")
  const failed = ["expired", "denied", "cancelled", "error"].includes(status)
  const key = errorCode && errorCode in STATUS_KEYS ? errorCode : status
  return (
    <div
      className={
        failed
          ? "flex w-full gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive"
          : "flex min-h-5 items-center gap-2 text-xs text-muted-foreground"
      }
    >
      {failed ? (
        <ShieldAlert className="h-4 w-4 shrink-0" />
      ) : status === "connected" ? (
        <CheckCircle2 className="h-4 w-4 text-emerald-500" />
      ) : (
        <Loader2 className="h-3.5 w-3.5 animate-spin" />
      )}
      <span>{error ?? t(statusKey(key))}</span>
    </div>
  )
}

function statusKey(key: string) {
  return STATUS_KEYS[key as keyof typeof STATUS_KEYS] ?? STATUS_KEYS.error
}
