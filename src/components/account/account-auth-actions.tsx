"use client"

import {
  ArrowLeft,
  ArrowRight,
  Check,
  Loader2,
  ShieldCheck,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { AccountFormScope } from "./account-auth-form"

export function BindingProgress({ props }: AccountFormScope) {
  const t = useTranslations("AccountFlow")
  const binding = props.mode === "binding"
  return binding ? (
    <div className="flex items-center gap-3 rounded-xl bg-primary/5 px-3 py-2.5 text-xs">
      <span className="flex size-6 items-center justify-center rounded-full bg-primary/10 text-primary">
        <Check className="size-3.5" />
      </span>
      <span className="text-primary">{t("wechatVerified")}</span>
      <span className="h-px flex-1 bg-primary/15" />
      <span>{t("bindPhone")}</span>
    </div>
  ) : null
}

export function AccountFormBack({ props }: AccountFormScope) {
  const t = useTranslations("AccountFlow")
  return props.mode === "register" || props.mode === "reset" ? (
    <Button
      type="button"
      variant="ghost"
      size="sm"
      className="h-7 justify-start px-0 text-xs text-muted-foreground"
      onClick={() => props.onMode("password")}
    >
      <ArrowLeft className="size-3" />
      {t("backLogin")}
    </Button>
  ) : null
}

export function AccountFormLinks({ props }: AccountFormScope) {
  const t = useTranslations("AccountFlow")
  const password = props.mode === "password"
  const binding = props.mode === "binding"
  return password ? (
    <div className="flex justify-between text-xs">
      <button
        type="button"
        className="text-primary hover:underline"
        onClick={() => props.onMode("register")}
      >
        {t("register")}
      </button>
      <button
        type="button"
        className="text-muted-foreground hover:text-foreground"
        onClick={() => props.onMode("reset")}
      >
        {t("forgotPassword")}
      </button>
    </div>
  ) : (
    <p className="-mt-2 text-[11px] leading-relaxed text-muted-foreground">
      {t(
        binding
          ? "bindingHint"
          : props.mode === "phone"
            ? "autoRegister"
            : "passwordRules"
      )}
    </p>
  )
}

export function AccountSubmitButton({ props, form }: AccountFormScope) {
  const t = useTranslations("AccountFlow")
  const binding = props.mode === "binding"
  return (
    <Button
      type="submit"
      className="h-10 w-full gap-2 rounded-xl"
      disabled={form.loading || form.security}
    >
      {form.loading ? <Loader2 className="size-4 animate-spin" /> : null}
      {t(
        binding
          ? "bindContinue"
          : props.mode === "reset"
            ? "resetPassword"
            : props.mode === "register"
              ? "registerContinue"
              : "loginContinue"
      )}
      {!form.loading ? <ArrowRight className="size-3.5" /> : null}
    </Button>
  )
}

export function AccountRescanButton({ props, form }: AccountFormScope) {
  const t = useTranslations("AccountFlow")
  const binding = props.mode === "binding"
  return binding ? (
    <Button
      type="button"
      variant="ghost"
      size="sm"
      disabled={form.loading}
      className="text-xs text-muted-foreground"
      onClick={() => props.onMode("wechat")}
    >
      <ArrowLeft className="size-3" />
      {t("rescan")}
    </Button>
  ) : null
}

export function AccountSecureCaption({}: AccountFormScope) {
  const t = useTranslations("AccountFlow")
  return (
    <div className="flex justify-center gap-1.5 text-[10px] text-muted-foreground">
      <ShieldCheck className="size-3" />
      {t("secureLogin")}
    </div>
  )
}
