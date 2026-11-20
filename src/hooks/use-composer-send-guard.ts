import { useCallback, useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { reportSendEvent } from "@/lib/send-diagnostics"

const SEND_CONFIRMATION_TIMEOUT_MS = 180_000

export function useComposerSendGuard() {
  const t = useTranslations("Folder.chat.messageInput")
  const [pending, setPending] = useState(false)
  const current = useRef<ReturnType<typeof setTimeout> | null>(null)
  const direct = useRef(false)

  const release = useCallback(() => {
    if (current.current === null) return
    clearTimeout(current.current)
    current.current = null
    direct.current = false
    setPending(false)
  }, [])

  useEffect(
    () => () => {
      if (current.current !== null) clearTimeout(current.current)
      current.current = null
    },
    []
  )

  const begin = useCallback(
    (isDirect = false) => {
      if (current.current !== null) return null
      const timer = setTimeout(() => {
        release()
        reportSendEvent("composer_timeout")
        toast.warning(t("confirmationTimedOut"))
      }, SEND_CONFIRMATION_TIMEOUT_MS)
      current.current = timer
      direct.current = isDirect
      setPending(true)
      return {
        active: () => current.current === timer,
        finish: () => {
          if (current.current === timer) release()
        },
      }
    },
    [release, t]
  )

  const finishTurn = useCallback(() => {
    if (direct.current) release()
  }, [release])
  return { pending, begin, finishTurn }
}
