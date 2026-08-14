"use client"

import { useCallback, useMemo } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useSkillInventory } from "@/hooks/use-skill-inventory"
import type { SkillMarketV2Detail, SkillMarketViewV2 } from "@/lib/skill-market"
import {
  buildSkillMarketActivation,
  isConnectorOnlyPlugin,
} from "@/lib/skill-market-activation"

export function useSkillMarketActivation({
  view,
  detailOpen,
  detail,
}: {
  view: SkillMarketViewV2
  detailOpen: boolean
  detail: SkillMarketV2Detail | null
}) {
  const t = useTranslations("SkillMarketV2")
  const detailNeedsInventory =
    detailOpen &&
    detail?.installState !== "not_installed" &&
    Boolean(detail && !isConnectorOnlyPlugin(detail))
  const inventory = useSkillInventory(
    view === "installed" || detailNeedsInventory
  )
  const activation = useMemo(
    () =>
      buildSkillMarketActivation(detail, inventory.snapshot, inventory.loading),
    [detail, inventory.loading, inventory.snapshot]
  )
  const setEnabled = useCallback(
    async (enabled: boolean) => {
      const result = await inventory.setActivations(activation.targets, enabled)
      const incomplete = result.blocked + result.failed
      if (incomplete) {
        toast.warning(t("detail.activation.partialToast"), {
          description: result.issues[0]
            ? t("detail.activation.resultWithIssue", {
                changed: result.changed,
                blocked: result.blocked,
                failed: result.failed,
                issue: result.issues[0],
              })
            : t("detail.activation.result", {
                changed: result.changed,
                blocked: result.blocked,
                failed: result.failed,
              }),
        })
        return
      }
      toast.success(
        t(
          enabled
            ? "detail.activation.enabledToast"
            : "detail.activation.disabledToast"
        )
      )
    },
    [activation.targets, inventory, t]
  )
  return {
    inventory,
    activation,
    activationError:
      activation.kind === "connector_only" ? null : inventory.error,
    activationBusy: inventory.busyKey === "bulk:activation",
    setEnabled,
  }
}
