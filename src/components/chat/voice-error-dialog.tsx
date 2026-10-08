"use client"

import { MicOff, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import type { RealtimeVoiceErrorKind } from "@/hooks/use-realtime-voice-input"

interface VoiceErrorDialogProps {
  kind: RealtimeVoiceErrorKind | null
  onRetry: () => void
  onOpenChange: (open: boolean) => void
}

export function VoiceErrorDialog({
  kind,
  onRetry,
  onOpenChange,
}: VoiceErrorDialogProps) {
  const t = useTranslations("Folder.chat.messageInput.voice")

  return (
    <Dialog open={kind !== null} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <MicOff className="size-5 text-destructive" />
            {t("errorTitle")}
          </DialogTitle>
          <DialogDescription>
            {kind ? t(kind) : t("serviceUnavailable")}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("dismiss")}
          </Button>
          <Button onClick={onRetry}>
            <RefreshCw />
            {t("retry")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
