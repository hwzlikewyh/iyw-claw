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
import type { IywLoginResult, IywWorkspace } from "@/lib/iyw-account-flow"
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
      const switched = await account.authenticate("switch_workspace", {
        orgCode: target.org_code,
      })
      const result = await recoverWorkspace(
        account,
        switched,
        t("serviceSyncFailed")
      )
      if (result.profile?.workspace_pending) setError(t("workspaceUnconfirmed"))
      else {
        if (result.profile?.points_stale) void account.refreshProfile()
        setTarget(null)
        if (!result.profile?.business_credentials_pending)
          toast.success(t("switched", { name: target.name }))
      }
    } catch (reason) {
      setError(toErrorMessage(reason))
    }
  }
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
      <AccountFlowError message={error} />
      <div className="-mx-2 -mb-2 flex items-center gap-1.5 rounded-b-xl border-t bg-muted/30 px-3 py-2.5 text-[10px] text-muted-foreground">
        <Folder className="size-3" />
        {t("localFilesKept")}
      </div>
    </PopoverContent>
  )
}

async function recoverWorkspace(
  account: WorkspaceScope["account"],
  result: IywLoginResult,
  failureMessage: string
) {
  const profile = result.profile
  if (!profile?.workspace_pending && !profile?.business_credentials_pending)
    return result
  console.info("[iyw-account] Automatically recovering workspace services", {
    workspacePending: profile.workspace_pending,
    businessCredentialsPending: profile.business_credentials_pending,
  })
  try {
    const recovered = await account.authenticate("recover_workspace")
    const pending = Boolean(
      recovered.profile?.workspace_pending ||
      recovered.profile?.business_credentials_pending
    )
    if (pending) {
      console.warn("[iyw-account] Workspace services are still pending")
      toast.error(failureMessage)
    } else {
      console.info("[iyw-account] Workspace services recovered")
    }
    return recovered
  } catch (reason) {
    console.warn("[iyw-account] Automatic workspace recovery failed", {
      message: toErrorMessage(reason),
    })
    toast.error(toErrorMessage(reason))
    return result
  }
}
