"use client"

/**
 * 新会话配置对账诊断面板 — 设置页展示最近一次新建/恢复会话的对账时间与
 * 结果。数据来自 `session_config_reconciler::diagnostics` 的脱敏快照
 * （不含配置正文 / token / key / 完整用户路径），用于"查看诊断 / 重试 /
 * 打开设置"入口的数据源。Mounted under `/settings/general`。
 */

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { CheckCircle2, Loader2, ShieldCheck, XCircle } from "lucide-react"

import {
  type SessionConfigReconcileDiagnostic,
  type SessionConfigReconcileDiagnostics,
  getSessionConfigReconcileDiagnostics,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"

function formatTime(iso: string): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  return date.toLocaleString()
}

function ReconcileRow({
  label,
  diagnostic,
}: {
  label: string
  diagnostic: SessionConfigReconcileDiagnostic | null
}) {
  const t = useTranslations("SessionConfigDiagnostics")
  return (
    <div className="rounded-md border bg-muted/20 p-3 space-y-1.5">
      <div className="flex items-center justify-between gap-3">
        <span className="text-sm font-medium">{label}</span>
        {diagnostic === null ? (
          <span className="text-xs text-muted-foreground">{t("none")}</span>
        ) : diagnostic.error_code ? (
          <span className="inline-flex items-center gap-1 text-xs text-destructive">
            <XCircle className="h-3.5 w-3.5" aria-hidden />
            {t("failed")}
          </span>
        ) : (
          <span className="inline-flex items-center gap-1 text-xs text-emerald-500">
            <CheckCircle2 className="h-3.5 w-3.5" aria-hidden />
            {t("ok")}
          </span>
        )}
      </div>
      {diagnostic === null ? (
        <p className="text-xs text-muted-foreground">{t("noneHint")}</p>
      ) : (
        <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs text-muted-foreground">
          <div className="flex justify-between gap-2">
            <dt>{t("time")}</dt>
            <dd className="text-foreground/90">
              {formatTime(diagnostic.occurred_at)}
            </dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt>{t("kind")}</dt>
            <dd className="text-foreground/90">
              {diagnostic.kind === "resume" ? t("kindResume") : t("kindNew")}
            </dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt>{t("schemaVersion")}</dt>
            <dd className="text-foreground/90">{diagnostic.schema_version}</dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt>{t("controlledFields")}</dt>
            <dd className="text-foreground/90">
              {diagnostic.controlled_fields}
            </dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt>{t("changed")}</dt>
            <dd className="text-foreground/90">
              {diagnostic.changed ? t("changedYes") : t("changedNo")}
            </dd>
          </div>
          <div className="flex justify-between gap-2">
            <dt>{t("durationMs")}</dt>
            <dd className="text-foreground/90">{diagnostic.duration_ms}</dd>
          </div>
          {diagnostic.fingerprint && (
            <div className="flex justify-between gap-2 col-span-2">
              <dt>{t("fingerprint")}</dt>
              <dd className="text-foreground/90 font-mono">
                {diagnostic.fingerprint.slice(0, 12)}…
              </dd>
            </div>
          )}
          {diagnostic.error_code && (
            <div className="flex justify-between gap-2 col-span-2">
              <dt>{t("errorCode")}</dt>
              <dd className="text-foreground/90 font-mono">
                {diagnostic.error_code}
              </dd>
            </div>
          )}
        </dl>
      )}
    </div>
  )
}

export function SessionConfigDiagnosticsSection() {
  const t = useTranslations("SessionConfigDiagnostics")
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [snapshot, setSnapshot] =
    useState<SessionConfigReconcileDiagnostics | null>(null)

  useEffect(() => {
    let cancelled = false
    void getSessionConfigReconcileDiagnostics()
      .then((value) => {
        if (cancelled) return
        setSnapshot(value)
        setLoadError(null)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setLoadError(toErrorMessage(err))
      })
      .finally(() => {
        if (cancelled) return
        setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <section className="rounded-xl border bg-card p-4 space-y-4">
      <div className="flex items-center gap-2">
        <ShieldCheck
          className="h-4 w-4 text-muted-foreground"
          aria-hidden
        />
        <h2 className="text-sm font-semibold">{t("title")}</h2>
      </div>
      <p className="text-xs text-muted-foreground leading-5">
        {t("description")}
      </p>

      {loadError && (
        <p className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          {t("loadFailed", { detail: loadError })}
        </p>
      )}

      {loading ? (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
          {t("loading")}
        </div>
      ) : (
        <div className="space-y-3">
          <ReconcileRow
            label={t("codexLabel")}
            diagnostic={snapshot?.codex ?? null}
          />
          <ReconcileRow
            label={t("claudeCodeLabel")}
            diagnostic={snapshot?.claude_code ?? null}
          />
        </div>
      )}
    </section>
  )
}
