"use client"

import { useState } from "react"
import { Building2, ChevronDown, Folder, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { toErrorMessage } from "@/lib/app-error"
import type { IywWorkspace } from "@/lib/iyw-account-flow"
import { WorkspaceRows } from "./account-workspace-rows"
import { AccountFlowError } from "./account-flow-feedback"
import { WorkspaceDialog } from "./account-workspace-dialog"

export interface WorkspaceScope {
  account: ReturnType<typeof useIywAccount>
  mobile: boolean
  open: boolean
  setOpen: (open: boolean) => void
  target: IywWorkspace | null
  setTarget: (target: IywWorkspace | null) => void
  error: string | null
  spaces: IywWorkspace[]
  select: (id: string) => void
  switchSpace: () => Promise<void>
  recover: () => Promise<void>
}
export function AccountWorkspaceSwitch({
  mobile = false,
}: {
  mobile?: boolean
}) {
  const scope = useWorkspaceSelection(mobile)
  if (!scope.account.profile?.logged_in) return null
  return (
    <>
      <WorkspacePopover {...scope} />
      <WorkspaceDialog {...scope} />
    </>
  )
}
function useWorkspaceSelection(mobile: boolean) {
  const t = useTranslations("AccountFlow")
  const account = useIywAccount()
  const [open, setOpen] = useState(false)
  const [target, setTarget] = useState<IywWorkspace | null>(null)
  const [error, setError] = useState<string | null>(null)
  const spaces = account.profile?.org_list ?? []
  const select = (id: string) => {
    if (id === account.profile?.org_code) return
    setTarget(spaces.find((space) => space.org_code === id) ?? null)
    setError(null)
    setOpen(false)
  }
  const switchSpace = async () => {
    if (!target) return
    setError(null)
    try {
      const result = await account.authenticate("switch_workspace", {
        orgCode: target.org_code,
      })
      if (result.profile?.workspace_pending) setError(t("workspaceUnconfirmed"))
      else {
        setTarget(null)
        toast.success(t("switched", { name: target.name }))
        if (result.profile?.business_credentials_pending) setOpen(true)
      }
    } catch (reason) {
      setError(toErrorMessage(reason))
    }
  }
  const recover = () => recoverWorkspace(account, setError)
  return {
    account,
    mobile,
    open,
    setOpen,
    target,
    setTarget,
    error,
    spaces,
    select,
    switchSpace,
    recover,
  }
}
function WorkspacePopover(scope: WorkspaceScope) {
  return (
    <Popover open={scope.open} onOpenChange={scope.setOpen}>
      <WorkspaceTrigger {...scope} />
      <WorkspacePopoverContent {...scope} />
    </Popover>
  )
}
function WorkspaceTrigger({ account, mobile }: WorkspaceScope) {
  const t = useTranslations("AccountFlow")
  return (
    <PopoverTrigger asChild>
      <Button
        variant="ghost"
        disabled={account.actionLoading}
        className={
          mobile
            ? "h-8 max-w-40 gap-1.5 rounded-lg px-2 text-xs"
            : "h-6 max-w-[188px] gap-1.5 rounded-md px-1.5 text-[11px]"
        }
        title={t("switchSpace")}
      >
        <Building2 className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="truncate">
          {account.profile?.workspace_pending
            ? t("spaceConfirming")
            : account.profile?.org_name || t("personalSpace")}
        </span>
        <ChevronDown className="size-3 shrink-0 text-muted-foreground" />
      </Button>
    </PopoverTrigger>
  )
}
function WorkspacePopoverContent({
  account,
  spaces,
  select,
  error,
  recover,
}: WorkspaceScope) {
  const t = useTranslations("AccountFlow")
  return (
    <PopoverContent
      align="end"
      sideOffset={8}
      className="w-[19rem] max-w-[calc(100vw-1.5rem)] gap-2 rounded-xl border p-2 shadow-lg"
    >
      <div className="border-b px-2.5 pb-3 pt-2">
        <h3 className="text-xs font-semibold">{t("switchSpace")}</h3>
        <p className="mt-1 text-[10px] text-muted-foreground">
          {t("spaceMenuHint")}
        </p>
      </div>
      <div className="max-h-[50dvh] overflow-y-auto">
        <WorkspaceRows
          spaces={spaces}
          current={account.profile?.org_code}
          onSelect={select}
          disabled={account.actionLoading}
        />
      </div>
      {!spaces.length ? (
        <Button
          variant="ghost"
          size="sm"
          disabled={account.actionLoading}
          onClick={() => void account.refreshProfile()}
        >
          <RefreshCw className="size-3.5" />
          {t("refreshSpaces")}
        </Button>
      ) : null}
      <WorkspaceSyncStatus account={account} recover={recover} />
      <AccountFlowError message={error} />
      <div className="-mx-2 -mb-2 flex items-center gap-1.5 rounded-b-xl border-t bg-muted/30 px-3 py-2.5 text-[10px] text-muted-foreground">
        <Folder className="size-3" />
        {t("localFilesKept")}
      </div>
    </PopoverContent>
  )
}

function WorkspaceSyncStatus({
  account,
  recover,
}: Pick<WorkspaceScope, "account" | "recover">) {
  const t = useTranslations("AccountFlow")
  return account.profile?.workspace_pending ||
    account.profile?.business_credentials_pending ? (
    <div className="rounded-lg border border-amber-200/70 bg-amber-50/50 p-3 text-xs text-amber-800 dark:border-amber-800/40 dark:bg-amber-950/20 dark:text-amber-200">
      <p>
        {t(
          account.profile?.workspace_pending
            ? "workspaceUnconfirmed"
            : "businessPending"
        )}
      </p>
      <Button
        variant="link"
        size="sm"
        className="mt-1 h-6 px-0 text-xs"
        disabled={account.actionLoading}
        onClick={() => void recover()}
      >
        {t("syncServices")}
      </Button>
    </div>
  ) : null
}

async function recoverWorkspace(
  account: WorkspaceScope["account"],
  setError: (error: string | null) => void
) {
  setError(null)
  try {
    await account.authenticate("recover_workspace")
  } catch (reason) {
    setError(toErrorMessage(reason))
  }
}
