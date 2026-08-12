"use client"

import { CheckIcon, LoaderCircle, Mic } from "lucide-react"
import { useTranslations } from "next-intl"
import { ContextMenu as ContextMenuPrimitive } from "radix-ui"

import { Button } from "@/components/ui/button"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import type { RealtimeVoiceStatus } from "@/hooks/use-realtime-voice-input"

interface RealtimeVoiceButtonProps {
  status: RealtimeVoiceStatus
  autoSend: boolean
  disabled: boolean
  onToggle: () => void
  onAutoSendChange: (value: boolean) => void
}

export function RealtimeVoiceButton({
  status,
  autoSend,
  disabled,
  onToggle,
  onAutoSendChange,
}: RealtimeVoiceButtonProps) {
  const t = useTranslations("Folder.chat.messageInput.voice")
  const title = t(statusTitleKey(status))

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <Button
          type="button"
          onClick={onToggle}
          disabled={disabled}
          variant={status === "recording" ? "destructive" : "ghost"}
          size="icon"
          className="h-8 w-8"
          title={title}
          aria-label={title}
          aria-pressed={status === "recording"}
          onContextMenu={(event) => event.stopPropagation()}
        >
          {status === "starting" || status === "stopping" ? (
            <LoaderCircle className="size-4 animate-spin" />
          ) : (
            <Mic className="size-4" />
          )}
        </Button>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuPrimitive.CheckboxItem
          checked={autoSend}
          className="focus:bg-accent focus:text-accent-foreground relative flex cursor-default items-center gap-2.5 rounded-xl py-2 pr-8 pl-3 text-sm outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-50"
          onCheckedChange={(checked) => onAutoSendChange(checked === true)}
        >
          {t("autoSend")}
          <span className="pointer-events-none absolute right-2 flex items-center justify-center">
            <ContextMenuPrimitive.ItemIndicator>
              <CheckIcon className="size-4" />
            </ContextMenuPrimitive.ItemIndicator>
          </span>
        </ContextMenuPrimitive.CheckboxItem>
      </ContextMenuContent>
    </ContextMenu>
  )
}

function statusTitleKey(
  status: RealtimeVoiceStatus
): "start" | "stop" | "starting" | "stopping" {
  if (status === "starting") return "starting"
  if (status === "recording") return "stop"
  if (status === "stopping") return "stopping"
  return "start"
}
