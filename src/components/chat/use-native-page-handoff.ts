import { useCallback } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useTabStore } from "@/stores/tab-store"
import {
  emitAppendTextToSession,
  emitAttachImageToSession,
} from "@/lib/session-attachment-events"
import type { PageCapture } from "@/lib/page-capture"
import type { NativePageModel } from "./use-native-page-model"

export function useNativePageHandoff(tabId: string, model: NativePageModel) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  const exists = useCallback(
    (target: string) => {
      if (useTabStore.getState().tabs.some((tab) => tab.id === target))
        return true
      toast.error(t("gone"))
      return false
    },
    [t]
  )
  const sendText = useCallback(
    (text: string) => {
      if (!exists(tabId)) return
      const accepted = emitAppendTextToSession({
        tabId,
        text: plainInput(`${t("untrusted")}\n${text}`),
      })
      toast[accepted ? "success" : "error"](
        t(accepted ? "added" : "inputUnavailable")
      )
    },
    [exists, tabId, t]
  )
  const sendCapture = (image: PageCapture, note: string, target: string) => {
    if (!exists(target)) throw new Error(t("gone"))
    const accepted = emitAttachImageToSession({
      tabId: target,
      ...image,
      name: "page-screenshot.png",
    })
    if (!accepted) throw new Error(t("inputUnavailable"))
    if (
      note &&
      !emitAppendTextToSession({ tabId: target, text: plainInput(note) })
    ) {
      throw new Error(t("inputUnavailable"))
    }
    model.setCapture(null)
    toast.success(t("added"))
  }
  return { sendText, sendCapture }
}

function plainInput(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
}
