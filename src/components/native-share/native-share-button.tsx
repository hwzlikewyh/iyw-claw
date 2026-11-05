"use client"

import { useEffect, useState, useSyncExternalStore } from "react"
import { MessageSquare } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { isDesktop } from "@/lib/transport"
import {
  NATIVE_SHARE_COUNT,
  nativeSharePendingCount,
  openNativeShareInbox,
} from "@/lib/native-share"

const subscribeDesktop = () => () => {}
const desktopOnServer = () => false

export function NativeShareButton() {
  const t = useTranslations("NativeShare")
  const available = useSyncExternalStore(
    subscribeDesktop,
    isDesktop,
    desktopOnServer
  )
  const [pending, setPending] = useState(nativeSharePendingCount)
  useEffect(() => {
    const count = (event: Event) =>
      setPending((event as CustomEvent<number>).detail)
    window.addEventListener(NATIVE_SHARE_COUNT, count)
    return () => window.removeEventListener(NATIVE_SHARE_COUNT, count)
  }, [])
  if (!available) return null
  const label = pending ? t("pendingCount", { count: pending }) : t("title")
  return (
    <Button
      variant="ghost"
      size="icon"
      title={label}
      aria-label={label}
      className="relative size-7 shrink-0 rounded-md text-muted-foreground"
      onClick={openNativeShareInbox}
    >
      <MessageSquare className="size-3.5" />
      {pending > 0 && (
        <span className="absolute -right-1 -top-1 min-w-3 rounded bg-muted px-0.5 text-[11px] tabular-nums text-foreground">
          {pending}
        </span>
      )}
    </Button>
  )
}
