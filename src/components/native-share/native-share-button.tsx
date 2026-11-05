"use client"

import { useEffect, useState, useSyncExternalStore } from "react"
import { MessageSquare } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { isDesktop } from "@/lib/transport"
import {
  NATIVE_SHARE_COUNT,
  nativeSharePendingCount,
  openNativeShareInbox,
  shareError,
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
      variant="outline"
      size="sm"
      title={label}
      aria-label={label}
      onClick={() =>
        void openNativeShareInbox().catch((error) =>
          toast.error(shareError(error))
        )
      }
    >
      <MessageSquare className="size-3.5" />
      {t("manageImports")}
      {pending > 0 && (
        <span className="rounded bg-muted px-1 text-xs tabular-nums text-foreground">
          {pending}
        </span>
      )}
    </Button>
  )
}
