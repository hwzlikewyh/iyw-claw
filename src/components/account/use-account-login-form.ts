"use client"

import { useCallback, useEffect, useRef, useState, type RefObject } from "react"
import { useTranslations } from "next-intl"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { toErrorMessage } from "@/lib/app-error"
import type { IywAuthAction } from "@/lib/iyw-account-flow"

export type AccountFormMode =
  | "phone"
  | "password"
  | "register"
  | "reset"
  | "binding"
const SMS_COOLDOWN_SECONDS = 60
const SECOND_MS = 1000
const EMPTY_VALUES = { phone: "", captcha: "", username: "", password: "" }
type Values = typeof EMPTY_VALUES
interface FormState {
  values: Values
  captchaKey: string | null
  remaining: number
  error: string | null
  security: boolean
}
interface FormScope {
  state: FormState
  update: (values: Partial<FormState>) => void
  account: ReturnType<typeof useIywAccount>
  mode: AccountFormMode
  onDone: () => void
  generation: RefObject<number>
  busy: RefObject<boolean>
  invalidPhone: string
  sendFirst: string
}

export function useAccountLoginForm(mode: AccountFormMode, onDone: () => void) {
  const t = useTranslations("AccountFlow")
  const account = useIywAccount()
  const [state, setState] = useState<FormState>({
    values: EMPTY_VALUES,
    captchaKey: null,
    remaining: 0,
    error: null,
    security: false,
  })
  const generation = useRef(0)
  const busy = useRef(false)
  const update = useCallback(
    (values: Partial<FormState>) =>
      setState((state) => ({ ...state, ...values })),
    []
  )
  const scope = {
    state,
    update,
    account,
    mode,
    onDone,
    generation,
    busy,
    invalidPhone: t("invalidPhone"),
    sendFirst: t("sendFirst"),
  }
  useFormLifecycle(scope)
  return {
    ...state,
    loading: account.actionLoading,
    setValue: (name: keyof Values, value: string) =>
      changeValue(scope, name, value),
    sendCode: () => sendCode(scope),
    submit: () => submit(scope),
  }
}

function useFormLifecycle({ state, update, generation }: FormScope) {
  useEffect(
    () => () => {
      generation.current += 1
    },
    [generation]
  )
  useEffect(() => {
    if (state.remaining <= 0) return
    const timer = window.setTimeout(
      () => update({ remaining: state.remaining - 1 }),
      SECOND_MS
    )
    return () => window.clearTimeout(timer)
  }, [state.remaining, update])
}

function changeValue(scope: FormScope, name: keyof Values, value: string) {
  const values = { ...scope.state.values, [name]: value }
  if (name === "phone") {
    scope.generation.current += 1
    values.captcha = ""
    scope.update({
      values,
      error: null,
      captchaKey: null,
      remaining: 0,
      security: false,
    })
  } else {
    scope.update({ values, error: null })
  }
}

async function sendCode(scope: FormScope) {
  const { state, account, mode, generation, busy, update } = scope
  if (busy.current || account.actionLoading || state.remaining > 0) return
  if (!/^1\d{10}$/.test(state.values.phone)) {
    update({ error: scope.invalidPhone })
    return
  }
  busy.current = true
  const current = generation.current
  update({ error: null })
  try {
    const result = await account.authenticate("phone_captcha", {
      phone: state.values.phone,
      type: mode === "reset" ? 3 : mode === "register" ? 4 : 1,
    })
    if (current !== generation.current) return
    update({
      security: result.security_verification_required,
      captchaKey: result.captcha_key,
      remaining:
        result.captcha_key && !result.security_verification_required
          ? SMS_COOLDOWN_SECONDS
          : 0,
    })
  } catch (reason) {
    if (current === generation.current)
      update({ error: toErrorMessage(reason) })
  } finally {
    busy.current = false
  }
}

async function submit(scope: FormScope) {
  const { state, account, mode, generation, busy, update } = scope
  if (busy.current || account.actionLoading) return
  if (mode !== "password" && !state.captchaKey) {
    update({ error: scope.sendFirst })
    return
  }
  const action: IywAuthAction =
    mode === "binding"
      ? "bind_phone"
      : mode === "reset"
        ? "reset_password"
        : mode
  busy.current = true
  const current = generation.current
  update({ error: null })
  try {
    const result = await account.authenticate(action, {
      ...state.values,
      captchaKey: state.captchaKey,
      flowId: account.loginFlow?.flow_id,
    })
    if (current !== generation.current) return
    update({ values: { ...state.values, password: "", captcha: "" } })
    if (result.status === "authenticated" || result.status === "password_reset")
      scope.onDone()
  } catch (reason) {
    if (current === generation.current)
      update({ error: toErrorMessage(reason) })
  } finally {
    busy.current = false
  }
}
