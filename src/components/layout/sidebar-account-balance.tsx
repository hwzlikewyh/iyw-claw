"use client"

import { ArrowUpRight, Building2, Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { balancePoints } from "@/components/account/account-profile-panel"
import { AccountPointsStatus } from "@/components/account/account-points-status"
import type { IywAccountProfile } from "@/lib/types"

interface Props {
  profile: IywAccountProfile
  pending: boolean
  refreshing: boolean
  onRefresh: () => void
  onProfile: () => void
}

export function SidebarAccountBalance(props: Props) {
  const t = useTranslations("SidebarAccount")
  const flow = useTranslations("AccountFlow")
  const details = useTranslations("SidebarDesign")
  return (
    <div className="mx-4 mb-2 grid gap-3">
      <div className="relative overflow-hidden rounded-2xl border border-primary/10 bg-gradient-to-br from-primary/[0.07] via-primary/[0.025] to-background p-4">
        <div className="flex items-center justify-between">
          <p className="text-[11px] font-medium text-muted-foreground">
            {props.profile.points_stale
              ? flow("lastConfirmedPoints")
              : flow("availablePoints")}
          </p>
          <Button
            variant="ghost"
            size="icon"
            className="size-6 rounded-lg text-muted-foreground"
            aria-label={t("refreshBalance")}
            disabled={props.pending || props.refreshing}
            onClick={props.onRefresh}
          >
            {props.refreshing ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <RefreshCw className="size-3" />
            )}
          </Button>
        </div>
        <div className="mt-2 flex items-baseline gap-1.5">
          <span className="text-[29px] font-semibold tabular-nums tracking-tight">
            {balancePoints(props.profile, t("balanceUnknown"))}
          </span>
          <span className="text-[10px] text-muted-foreground">
            {flow("pointsUnit")}
          </span>
        </div>
        <div className="mt-3 flex min-w-0 items-center justify-between gap-3 border-t border-primary/[0.08] pt-3">
          <span className="flex min-w-0 items-center gap-1.5 text-[10px] text-muted-foreground">
            <Building2 className="size-3 shrink-0" />
            <span className="truncate">
              {props.profile.org_name || flow("personalSpace")}
            </span>
          </span>
          <button
            type="button"
            onClick={props.onProfile}
            className="inline-flex shrink-0 items-center gap-0.5 text-[10px] text-primary/80 hover:text-primary"
          >
            {details("accountProfile")}
            <ArrowUpRight className="size-2.5" />
          </button>
        </div>
        {props.profile.balance_expiry_time ? (
          <p className="mt-2 text-[10px] text-muted-foreground">
            {t("balanceExpiry")}: {props.profile.balance_expiry_time}
          </p>
        ) : null}
      </div>
      <AccountPointsStatus profile={props.profile} />
    </div>
  )
}
