"use client"

import { ShieldAlert } from "lucide-react"
import { useTranslations } from "next-intl"
import ReactMarkdown from "react-markdown"
import remarkGfm from "remark-gfm"

import type { UpdateDetails } from "@/components/layout/title-bar-update-model"
import type { AppUpdateInfo } from "@/lib/updater"

export function TitleBarUpdateReleaseDetails({
  update,
  details,
}: {
  update: AppUpdateInfo | null
  details: UpdateDetails
}) {
  const t = useTranslations("SystemSettings")
  if (!update) return null
  return (
    <div className="space-y-3 border-t pt-4">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="font-medium text-foreground">
          {t("upgradableVersion")}: v{update.version}
        </span>
        {update.channel ? (
          <span className="rounded border px-1.5 py-0.5 text-muted-foreground">
            {t(update.channel === "beta" ? "betaChannel" : "stableChannel")}
          </span>
        ) : null}
        {update.updatePolicy === "required" ? (
          <span className="inline-flex items-center gap-1 rounded border border-destructive/40 px-1.5 py-0.5 text-destructive">
            <ShieldAlert className="size-3" />
            {t("requiredUpdate")}
          </span>
        ) : null}
      </div>
      <div className="max-h-56 overflow-y-auto border-l-2 pl-3 text-xs leading-6 text-muted-foreground [&_a]:text-primary [&_a]:underline [&_li]:mb-1 [&_ol]:list-decimal [&_ol]:pl-5 [&_p]:mb-2 [&_ul]:list-disc [&_ul]:pl-5">
        {update.body ? (
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {update.body}
          </ReactMarkdown>
        ) : (
          t("none")
        )}
      </div>
      {details.runtime === "docker" ? (
        <p className="text-xs leading-5 text-muted-foreground">
          {t("dockerUpgradeHint")}
        </p>
      ) : null}
    </div>
  )
}
