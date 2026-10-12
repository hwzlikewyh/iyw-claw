"use client"

import { AlertCircle, ExternalLink, ShieldCheck } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
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

export function AccountSecurityRequired({ binding }: { binding: boolean }) {
  const t = useTranslations("AccountFlow")
  return (
    <div
      role="status"
      className="rounded-xl border border-amber-200/70 bg-amber-50/70 p-3 text-xs leading-relaxed text-amber-800 dark:border-amber-800/50 dark:bg-amber-950/30 dark:text-amber-200"
    >
      <div className="flex gap-2">
        <ShieldCheck className="mt-0.5 size-3.5 shrink-0" />
        <p>{t(binding ? "bindingSecurity" : "smsSecurity")}</p>
      </div>
      <Button
        type="button"
        variant="link"
        size="sm"
        className="mt-1 h-7 px-0 text-xs"
        onClick={() => void openExternal("https://account.iyw.cn")}
      >
        {t("officialAccount")}
        <ExternalLink className="size-3" />
      </Button>
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
