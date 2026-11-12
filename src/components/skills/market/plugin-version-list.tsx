"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { usePluginVersions } from "./use-plugin-versions"

export function PluginVersionListRows({
  state,
}: {
  state: ReturnType<typeof usePluginVersions>
}) {
  const t = useTranslations("PluginVersions")
  if (state.loading)
    return (
      <p role="status" className="text-sm">
        {t("loading")}
      </p>
    )
  if (!state.list) return null
  return (
    <div className="space-y-3">
      <p className="break-words text-sm font-medium">
        {state.list.slug} ·{" "}
        {t("current", { version: state.list.currentVersion })}
      </p>
      {!state.list.versions.some((version) => !version.current) ? (
        <p className="text-sm text-muted-foreground">{t("empty")}</p>
      ) : null}
      <VersionRows state={state} />
      <VersionPages state={state} />
    </div>
  )
}

function VersionRows({
  state,
}: {
  state: ReturnType<typeof usePluginVersions>
}) {
  const t = useTranslations("PluginVersions")
  if (!state.list) return null
  return (
    <ul className="divide-y">
      {state.list.versions.map((version) => (
        <li
          key={version.version}
          className="flex flex-wrap items-start justify-between gap-2 py-3 text-sm"
        >
          <div className="min-w-0 flex-1">
            <p className="break-words">
              {version.version}
              {version.current ? ` · ${t("currentLabel")}` : ""}
            </p>
            {version.issue ? (
              <p className="break-words text-muted-foreground">
                {version.issue}
              </p>
            ) : null}
          </div>
          {!version.current ? (
            <Button
              size="sm"
              variant="outline"
              disabled={state.busy || !version.available}
              onClick={() => void state.inspect(version.version)}
            >
              {t("inspect")}
            </Button>
          ) : null}
        </li>
      ))}
    </ul>
  )
}

function VersionPages({
  state,
}: {
  state: ReturnType<typeof usePluginVersions>
}) {
  const t = useTranslations("PluginVersions")
  if (!state.list) return null
  return (
    <div className="flex items-center justify-between gap-2">
      <Button
        size="sm"
        variant="outline"
        disabled={state.page <= 1 || state.busy}
        onClick={() => void state.load(state.page - 1)}
      >
        {t("previous")}
      </Button>
      <span className="text-sm text-muted-foreground">
        {t("page", { page: state.page })}
      </span>
      <Button
        size="sm"
        variant="outline"
        disabled={state.list.nextPage === null || state.busy}
        onClick={() => {
          if (state.list && state.list.nextPage !== null)
            void state.load(state.list.nextPage)
        }}
      >
        {t("next")}
      </Button>
    </div>
  )
}
