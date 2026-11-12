"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import type { PluginRemovalReview } from "@/lib/plugin-remove"

interface Props {
  view: PluginRemovalReview
  disabled: boolean
  onNavigate: () => void
}
export function PluginRemoveDetails({ view, disabled, onNavigate }: Props) {
  const t = useTranslations("PluginRemoval")
  return (
    <div className="space-y-3 text-sm">
      <p role="status" className="font-medium">
        {t(`state.${view.state}`)}
      </p>
      <p className="break-words">
        {view.slug}
        {view.version ? ` · ${view.version}` : ""}
      </p>
      {view.state === "ready" ? (
        <>
          <p>{t("affectedHint")}</p>
          <CapabilityLinks
            view={view}
            disabled={disabled}
            onNavigate={onNavigate}
          />
          <p>{t("retainedHint")}</p>
          <RetainedList
            view={view}
            disabled={disabled}
            onNavigate={onNavigate}
          />
        </>
      ) : null}
      {view.warnings.length ? (
        <ul className="space-y-1 text-muted-foreground">
          {view.warnings.map((warning, index) => (
            <li key={index} className="break-words">
              {warning}
            </li>
          ))}
        </ul>
      ) : null}
      {view.legacyRecordsRetained ? (
        <p className="text-muted-foreground">{t("legacyRetained")}</p>
      ) : null}
      <p className="text-muted-foreground">{t("accountHint")}</p>
    </div>
  )
}

function CapabilityLinks({ view, disabled, onNavigate }: Props) {
  const t = useTranslations("PluginRemoval"),
    route = useWorkbenchRoute()
  return view.affected.length ? (
    <ul>
      {view.affected.map((item) => (
        <li key={item.id}>
          <Button
            disabled={disabled}
            size="xs"
            variant="link"
            className="h-auto whitespace-normal p-0 text-left"
            onClick={() => {
              onNavigate()
              route.openSkillMarket(item.slug)
            }}
          >
            {item.slug}
          </Button>
        </li>
      ))}
    </ul>
  ) : (
    <p className="text-muted-foreground">{t("noAffected")}</p>
  )
}

function RetainedList({ view, disabled, onNavigate }: Props) {
  const t = useTranslations("PluginRemoval"),
    route = useWorkbenchRoute()
  return view.retained.length ? (
    <ul className="space-y-1">
      {view.retained.map((item) => (
        <li key={item.id} className="flex flex-wrap justify-between gap-2">
          <Button
            disabled={disabled}
            size="xs"
            variant="link"
            className="h-auto whitespace-normal p-0 text-left"
            onClick={() => {
              onNavigate()
              route.openSkillMarket(item.slug)
            }}
          >
            {item.slug}
          </Button>
          <span className="text-muted-foreground">
            {t(item.shared ? "shared" : "retainedUnused")}
          </span>
        </li>
      ))}
    </ul>
  ) : (
    <p className="text-muted-foreground">{t("noRetained")}</p>
  )
}
