"use client"

import { ArrowDown, ArrowRight, Building2, Folder, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { AccountFlowError } from "./account-flow-feedback"
import type { WorkspaceScope } from "./account-workspace-switch"

export function WorkspaceDialog({
  account,
  target,
  setTarget,
  error,
  switchSpace,
}: WorkspaceScope) {
  const t = useTranslations("AccountFlow")
  return (
    <Dialog
      open={target !== null}
      onOpenChange={(value) => {
        if (!value && !account.actionLoading) setTarget(null)
      }}
    >
      <DialogContent
        className="max-w-sm gap-5 rounded-2xl p-5 sm:p-6"
        onInteractOutside={(event) => {
          if (account.actionLoading) event.preventDefault()
        }}
        onEscapeKeyDown={(event) => {
          if (account.actionLoading) event.preventDefault()
        }}
      >
        <DialogHeader>
          <DialogTitle>{t("confirmSwitch")}</DialogTitle>
          <DialogDescription>{t("confirmSwitchHint")}</DialogDescription>
        </DialogHeader>
        <WorkspaceTransition account={account} target={target} />
        <AccountFlowError message={error} />
        <WorkspaceConfirmationActions
          account={account}
          setTarget={setTarget}
          switchSpace={switchSpace}
        />
      </DialogContent>
    </Dialog>
  )
}
function SpaceSummary({ name, label }: { name: string; label: string }) {
  return (
    <div className="flex items-center gap-3 rounded-xl border bg-muted/25 p-3">
      <span className="flex size-9 shrink-0 items-center justify-center rounded-xl bg-primary/7 text-primary">
        <Building2 className="size-4" />
      </span>
      <div className="min-w-0">
        <p className="truncate text-xs font-medium">{name}</p>
        <p className="mt-1 text-[10px] text-muted-foreground">{label}</p>
      </div>
    </div>
  )
}

function WorkspaceTransition({
  account,
  target,
}: Pick<WorkspaceScope, "account" | "target">) {
  const t = useTranslations("AccountFlow")
  return (
    <div className="grid gap-3">
      <SpaceSummary
        name={account.profile?.org_name || t("personalSpace")}
        label={t("currentSpace")}
      />
      <ArrowDown className="mx-auto size-4 text-muted-foreground/60" />
      <SpaceSummary name={target?.name ?? ""} label={t("targetSpace")} />
      <p className="flex gap-2 text-[11px] leading-relaxed text-muted-foreground">
        <Folder className="mt-0.5 size-3.5 shrink-0" />
        {t("localFilesKept")}
      </p>
    </div>
  )
}
function WorkspaceConfirmationActions({
  account,
  setTarget,
  switchSpace,
}: Pick<WorkspaceScope, "account" | "setTarget" | "switchSpace">) {
  const t = useTranslations("AccountFlow")
  return (
    <DialogFooter>
      <Button
        variant="outline"
        className="rounded-xl"
        disabled={account.actionLoading}
        onClick={() => setTarget(null)}
      >
        {t("cancel")}
      </Button>
      <Button
        className="rounded-xl"
        disabled={account.actionLoading}
        onClick={() => void switchSpace()}
      >
        {account.actionLoading ? (
          <Loader2 className="size-4 animate-spin" />
        ) : (
          <ArrowRight className="size-3.5" />
        )}
        {t("confirm")}
      </Button>
    </DialogFooter>
  )
}
