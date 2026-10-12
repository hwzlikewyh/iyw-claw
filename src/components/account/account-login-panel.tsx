"use client"

import { useCallback, useState } from "react"
import { useTranslations } from "next-intl"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { cn } from "@/lib/utils"
import { toErrorMessage } from "@/lib/app-error"
import { AccountAuthForm } from "./account-auth-form"
import { AccountFlowError } from "./account-flow-feedback"
import { AccountLoginSteps } from "./account-login-steps"
import { AccountWechatLogin } from "./account-wechat-login"
import type { AccountFormMode } from "./use-account-login-form"

type Mode = "wechat" | AccountFormMode

interface LoginScope {
  binding: boolean
  loginFlow: ReturnType<typeof useIywAccount>["loginFlow"]
  auxiliary: boolean
  mode: Mode
  actionLoading: boolean
  changeMode: (mode: Mode) => Promise<void>
  active: boolean
  complete: () => void
  setMode: (mode: Mode) => void
  setNotice: (notice: boolean) => void
}

export function AccountLoginPanel({
  active = true,
  onAuthenticated,
}: {
  active?: boolean
  onAuthenticated?: () => void
}) {
  const t = useTranslations("AccountFlow")
  const { loginFlow, cancelLogin, actionLoading } = useIywAccount()
  const [mode, setMode] = useState<Mode>("wechat")
  const [notice, setNotice] = useState(false)
  const [modeError, setModeError] = useState<string | null>(null)
  const complete = useCallback(() => onAuthenticated?.(), [onAuthenticated])
  const changeMode = async (next: Mode) => {
    setModeError(null)
    try {
      await cancelLogin()
      setNotice(false)
      setMode(next)
    } catch (reason) {
      setModeError(toErrorMessage(reason))
    }
  }
  const binding = loginFlow?.status === "phone_binding_required"
  const auxiliary = mode === "register" || mode === "reset"
  const scope = {
    binding,
    loginFlow,
    auxiliary,
    mode,
    actionLoading,
    changeMode,
    active,
    complete,
    setMode,
    setNotice,
  }
  return (
    <div className="p-5 sm:p-6">
      <AccountLoginHeader {...scope} />
      <AccountFlowError message={modeError} />
      {notice ? (
        <p
          role="status"
          className="mb-4 rounded-xl bg-primary/5 p-3 text-xs text-primary"
        >
          {t("passwordResetDone")}
        </p>
      ) : null}
      <AccountLoginBody {...scope} />
    </div>
  )
}

function AccountLoginHeader({
  binding,
  loginFlow,
  auxiliary,
  mode,
  actionLoading,
  changeMode,
}: LoginScope) {
  const t = useTranslations("AccountFlow")
  return binding ? (
    <div className="mb-5">
      <h3 className="text-lg font-semibold">{t("bindingTitle")}</h3>
      <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
        {t("bindingDescription")}
      </p>
    </div>
  ) : loginFlow ? null : auxiliary ? (
    <div className="mb-5">
      <h3 className="text-lg font-semibold">
        {t(mode === "register" ? "register" : "resetPassword")}
      </h3>
    </div>
  ) : (
    <AccountLoginTabs
      mode={mode}
      actionLoading={actionLoading}
      changeMode={changeMode}
    />
  )
}

function AccountLoginTabs({
  mode,
  actionLoading,
  changeMode,
}: Pick<LoginScope, "mode" | "actionLoading" | "changeMode">) {
  const t = useTranslations("AccountFlow")
  return (
    <div
      className="mb-6 grid grid-cols-3 gap-1 rounded-xl bg-muted/60 p-1"
      role="tablist"
      aria-label={t("loginMethod")}
    >
      {(["wechat", "phone", "password"] as const).map((item) => (
        <button
          key={item}
          type="button"
          role="tab"
          aria-selected={mode === item}
          disabled={actionLoading}
          onClick={() => void changeMode(item)}
          className={cn(
            "h-8 rounded-lg text-xs transition-colors",
            mode === item
              ? "bg-background font-medium text-foreground shadow-sm"
              : "text-muted-foreground hover:text-foreground"
          )}
        >
          {t(
            item === "wechat"
              ? "wechatTab"
              : item === "phone"
                ? "phoneTab"
                : "passwordTab"
          )}
        </button>
      ))}
    </div>
  )
}

function AccountLoginBody({
  binding,
  loginFlow,
  mode,
  active,
  complete,
  changeMode,
  setMode,
  setNotice,
}: LoginScope) {
  return binding ? (
    <AccountAuthForm
      key={loginFlow?.flow_id}
      mode="binding"
      onMode={(next) => void changeMode(next)}
      onDone={complete}
    />
  ) : loginFlow ? (
    <AccountLoginSteps
      key={loginFlow.flow_id}
      flow={loginFlow}
      onDone={complete}
    />
  ) : mode === "wechat" ? (
    <AccountWechatLogin active={active} onDone={complete} />
  ) : (
    <AccountAuthForm
      key={mode}
      mode={mode}
      onMode={(next) => void changeMode(next)}
      onDone={() => {
        if (mode === "reset") {
          setMode("password")
          setNotice(true)
        } else complete()
      }}
    />
  )
}
