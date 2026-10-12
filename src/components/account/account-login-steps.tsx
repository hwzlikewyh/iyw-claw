"use client"

import { useState } from "react"
import {
  ArrowLeft,
  ArrowRight,
  BriefcaseBusiness,
  Check,
  Loader2,
  Palette,
  UserRound,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { toErrorMessage } from "@/lib/app-error"
import { cn } from "@/lib/utils"
import type { IywAuthAction, IywLoginResult } from "@/lib/iyw-account-flow"
import { AccountFlowError } from "./account-flow-feedback"
import { WorkspaceRows } from "./account-workspace-rows"

interface LoginStepScope {
  flow: IywLoginResult
  identity: number
  setIdentity: (identity: number) => void
  selected: string
  setSelected: (id: string) => void
  actionLoading: boolean
  noSpace: boolean
  choosingIdentity: boolean
  run: (
    action: IywAuthAction,
    values?: Record<string, unknown>
  ) => Promise<void>
}
export function AccountLoginSteps({
  flow,
  onDone,
}: {
  flow: IywLoginResult
  onDone: () => void
}) {
  const { authenticate, cancelLogin, actionLoading } = useIywAccount()
  const [identity, setIdentity] = useState(0)
  const [selected, setSelected] = useState("")
  const [error, setError] = useState<string | null>(null)
  const run = async (
    action: IywAuthAction,
    values: Record<string, unknown> = {}
  ) => {
    setError(null)
    try {
      const result = await authenticate(action, {
        ...values,
        flowId: flow.flow_id,
      })
      if (result.status === "authenticated") onDone()
    } catch (reason) {
      setError(toErrorMessage(reason))
    }
  }
  const noSpace = flow.status === "no_workspace"
  const choosingIdentity = flow.status === "identity_required"
  const scope = {
    flow,
    identity,
    setIdentity,
    selected,
    setSelected,
    actionLoading,
    noSpace,
    choosingIdentity,
    run,
  }
  return (
    <div className="grid gap-5">
      <LoginStepHeading {...scope} />
      <LoginStepChoices {...scope} />
      <AccountFlowError message={error} />
      <LoginStepContinue {...scope} />
      <LoginOtherAccount
        loading={actionLoading}
        onBack={() =>
          void cancelLogin().catch((reason) => setError(toErrorMessage(reason)))
        }
      />
    </div>
  )
}
function LoginStepHeading({ noSpace, choosingIdentity }: LoginStepScope) {
  const t = useTranslations("AccountFlow")
  return (
    <div>
      <span className="mb-3 inline-flex size-10 items-center justify-center rounded-xl bg-primary/8 text-primary">
        {choosingIdentity ? (
          <Palette className="size-5" />
        ) : (
          <BriefcaseBusiness className="size-5" />
        )}
      </span>
      <h3 className="text-lg font-semibold">
        {t(
          choosingIdentity
            ? "identityTitle"
            : noSpace
              ? "noSpaceTitle"
              : "chooseSpace"
        )}
      </h3>
      <p className="mt-1.5 text-xs leading-relaxed text-muted-foreground">
        {t(
          choosingIdentity
            ? "identityHint"
            : noSpace
              ? "noSpaceHint"
              : "chooseSpaceHint"
        )}
      </p>
    </div>
  )
}

function LoginStepChoices({
  flow,
  identity,
  setIdentity,
  selected,
  setSelected,
  actionLoading,
  noSpace,
  choosingIdentity,
}: LoginStepScope) {
  const t = useTranslations("AccountFlow")
  return choosingIdentity ? (
    <div className="grid grid-cols-2 gap-3">
      {([0, 1] as const).map((value) => (
        <IdentityOption
          key={value}
          value={value}
          identity={identity}
          setIdentity={setIdentity}
          actionLoading={actionLoading}
        />
      ))}
    </div>
  ) : !noSpace ? (
    <WorkspaceRows
      spaces={flow.workspaces}
      selected={selected}
      onSelect={setSelected}
      disabled={actionLoading}
    />
  ) : (
    <div className="flex items-center gap-3 rounded-xl border bg-muted/20 p-4">
      <UserRound className="size-5 text-primary" />
      <span className="text-sm">{t("personalSpace")}</span>
    </div>
  )
}

function LoginStepContinue({
  actionLoading,
  choosingIdentity,
  noSpace,
  selected,
  identity,
  run,
}: LoginStepScope) {
  const t = useTranslations("AccountFlow")
  return (
    <Button
      className="h-10 rounded-xl"
      disabled={actionLoading || (!choosingIdentity && !noSpace && !selected)}
      onClick={() =>
        void run(
          choosingIdentity
            ? "set_identity"
            : noSpace
              ? "create_personal"
              : "select_workspace",
          choosingIdentity ? { identity } : { orgCode: selected }
        )
      }
    >
      {actionLoading ? <Loader2 className="size-4 animate-spin" /> : null}
      {t(noSpace ? "createAndContinue" : "continue")}
      {!actionLoading ? <ArrowRight className="size-3.5" /> : null}
    </Button>
  )
}

function IdentityOption({
  value,
  identity,
  setIdentity,
  actionLoading,
}: Pick<LoginStepScope, "identity" | "setIdentity" | "actionLoading"> & {
  value: 0 | 1
}) {
  const t = useTranslations("AccountFlow")
  return (
    <button
      key={value}
      type="button"
      disabled={actionLoading}
      aria-pressed={identity === value}
      onClick={() => setIdentity(value)}
      className={cn(
        "relative grid gap-3 rounded-xl border p-4 text-left transition-colors",
        identity === value
          ? "border-primary/50 bg-primary/5"
          : "hover:bg-muted/40"
      )}
    >
      <span className="text-primary">
        {value ? (
          <Palette className="size-5" />
        ) : (
          <UserRound className="size-5" />
        )}
      </span>
      <span className="text-sm font-medium">
        {t(value ? "designer" : "customer")}
      </span>
      <span className="text-[11px] leading-relaxed text-muted-foreground">
        {t(value ? "designerHint" : "customerHint")}
      </span>
      {identity === value ? (
        <Check className="absolute right-3 top-3 size-3.5 text-primary" />
      ) : null}
    </button>
  )
}

function LoginOtherAccount({
  loading,
  onBack,
}: {
  loading: boolean
  onBack: () => void
}) {
  const t = useTranslations("AccountFlow")
  return (
    <Button
      variant="ghost"
      size="sm"
      className="text-xs text-muted-foreground"
      disabled={loading}
      onClick={onBack}
    >
      <ArrowLeft className="size-3" />
      {t("otherAccount")}
    </Button>
  )
}
