"use client"

import { useTranslations } from "next-intl"
import styles from "./queued-send-status.module.css"

export function QueuedSendStatus({ count }: { count: number }) {
  const t = useTranslations("Folder.chat.chatInput")

  return (
    <div role="status" className={styles.status}>
      <span aria-hidden="true" className={styles.activity}>
        <span />
        <span />
        <span />
      </span>
      <span className={styles.message}>{t("queuedMessages", { count })}</span>
      <span aria-hidden="true" className={styles.hint}>
        {t("queuedKeepTyping")}
      </span>
    </div>
  )
}
