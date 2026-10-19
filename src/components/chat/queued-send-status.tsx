"use client"

import { useTranslations } from "next-intl"
import { Clock3 } from "lucide-react"
import styles from "./queued-send-status.module.css"

export function QueuedSendStatus({ count }: { count: number }) {
  const t = useTranslations("Folder.chat.chatInput")

  return (
    <div role="status" className={styles.status}>
      <Clock3 aria-hidden="true" className="size-3 shrink-0" />
      <span className={styles.message}>{t("queuedMessages", { count })}</span>
      <span aria-hidden="true" className={styles.hint}>
        {t("queuedKeepTyping")}
      </span>
    </div>
  )
}
