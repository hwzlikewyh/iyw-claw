"use client"

import { AlertCircle } from "lucide-react"
import { useTranslations } from "next-intl"
import { openUrl as openExternal } from "@/lib/platform"

export function AccountFlowError({ message }: { message: string | null }) {
  if (!message) return null
  return (
    <div
      role="alert"
      className="flex gap-2 rounded-xl border border-destructive/20 bg-destructive/5 px-3 py-2.5 text-xs leading-relaxed text-destructive"
    >
      <AlertCircle className="mt-0.5 size-3.5 shrink-0" />
      <span className="min-w-0 break-words">{message}</span>
    </div>
  )
}

export function AccountAgreement() {
  const t = useTranslations("AccountFlow")
  return (
    <p className="text-center text-[11px] leading-relaxed text-muted-foreground">
      {t("agreementPrefix")}{" "}
      <button
        type="button"
        className="text-primary hover:underline"
        onClick={() =>
          void openExternal("https://account.iyw.cn/protocol/platform.html")
        }
      >
        {t("agreement")}
      </button>{" "}
      {t("agreementJoin")}{" "}
      <button
        type="button"
        className="text-primary hover:underline"
        onClick={() =>
          void openExternal("https://account.iyw.cn/protocol/privacy.html")
        }
      >
        {t("privacy")}
      </button>
    </p>
  )
}
