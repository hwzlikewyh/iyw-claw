"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import type { PluginVersionReview } from "@/lib/plugin-versions"

export function PluginVersionReviewDetails({
  view,
  busy,
  onNavigate,
}: {
  view: PluginVersionReview
  busy: boolean
  onNavigate: () => void
}) {
  const t = useTranslations("PluginVersions")
  return (
    <div className="space-y-3 text-sm">
      <p role="status" className="font-medium">
        {t(`state.${view.state}`)}
      </p>
      <p className="break-words">
        {view.slug} · {view.currentVersion} → {view.version}
      </p>
      {view.listed === false ? <p>{t("unlisted")}</p> : null}
      <p>{t(view.scopeChanged ? "scopeChanged" : "sameScope")}</p>
      <p>
        {t(view.permissionsChanged ? "permissionsChanged" : "keepPermissions")}
      </p>
      {view.preparationChanged ? <p>{t("preparationChanged")}</p> : null}
      <p className="text-muted-foreground">{t("keepData")}</p>
      {view.issues.length ? (
        <ul className="space-y-1 text-destructive">
          {view.issues.map((issue, index) => (
            <li className="break-words" key={index}>
              {issue}
            </li>
          ))}
        </ul>
      ) : null}
      <VersionDependencies view={view} busy={busy} onNavigate={onNavigate} />
      <RelatedCapabilities view={view} busy={busy} onNavigate={onNavigate} />
    </div>
  )
}

function RelatedCapabilities({
  view,
  busy,
  onNavigate,
}: {
  view: PluginVersionReview
  busy: boolean
  onNavigate: () => void
}) {
  const t = useTranslations("PluginVersions"),
    route = useWorkbenchRoute()
  if (!view.related.length) return null
  return (
    <div className="space-y-2">
      <p>{t("related")}</p>
      <ul>
        {view.related.map((capability) => (
          <li key={capability.id}>
            <Button
              variant="link"
              size="xs"
              disabled={busy}
              className="h-auto whitespace-normal p-0 text-left"
              onClick={() => {
                onNavigate()
                route.openSkillMarket(capability.slug, capability.packageType)
              }}
            >
              {capability.slug}
            </Button>
          </li>
        ))}
      </ul>
    </div>
  )
}

function VersionDependencies({
  view,
  busy,
  onNavigate,
}: {
  view: PluginVersionReview
  busy: boolean
  onNavigate: () => void
}) {
  const t = useTranslations("PluginVersions"),
    route = useWorkbenchRoute()
  return (
    <ul className="space-y-2">
      {view.dependencies.map((dependency) => (
        <li
          key={dependency.id}
          className="flex flex-wrap items-start justify-between gap-2 text-sm"
        >
          <Button
            variant="link"
            size="xs"
            disabled={busy}
            className="h-auto whitespace-normal p-0 text-left"
            onClick={() => {
              onNavigate()
              route.openSkillMarket(dependency.slug, dependency.packageType)
            }}
          >
            {dependency.slug} · {dependency.version}
          </Button>
          <span
            className={
              dependency.ready ? "text-muted-foreground" : "text-destructive"
            }
          >
            {t(
              !dependency.ready
                ? "dependencyBlocked"
                : dependency.managed
                  ? "dependencyManaged"
                  : dependency.changing
                    ? "dependencyChanging"
                    : "dependencyRetained"
            )}
          </span>
        </li>
      ))}
    </ul>
  )
}
