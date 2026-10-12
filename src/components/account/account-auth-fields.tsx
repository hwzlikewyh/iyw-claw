"use client"

import { Eye, EyeOff } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import type { AccountFormScope } from "./account-auth-form"

export function AccountIdentityField({ form, props }: AccountFormScope) {
  const t = useTranslations("AccountFlow")
  const password = props.mode === "password"
  return password ? (
    <div className="grid gap-2">
      <Label htmlFor="account-username">{t("username")}</Label>
      <Input
        id="account-username"
        autoComplete="username"
        placeholder={t("usernameHint")}
        required
        value={form.values.username}
        onChange={(event) => form.setValue("username", event.target.value)}
        disabled={form.loading}
      />
    </div>
  ) : (
    <div className="grid gap-2">
      <Label htmlFor="account-phone">{t("phone")}</Label>
      <div className="relative">
        <span className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 border-r pr-2 text-xs text-muted-foreground">
          +86
        </span>
        <Input
          id="account-phone"
          className="pl-14"
          type="tel"
          inputMode="numeric"
          autoComplete="tel-national"
          maxLength={11}
          required
          placeholder={t("phoneHint")}
          value={form.values.phone}
          onChange={(event) =>
            form.setValue("phone", event.target.value.replace(/\D/g, ""))
          }
          disabled={form.loading}
        />
      </div>
    </div>
  )
}

export function AccountCaptchaField({ form, props }: AccountFormScope) {
  const t = useTranslations("AccountFlow")
  const password = props.mode === "password"
  return !password ? (
    <div className="grid gap-2">
      <div className="flex items-center justify-between">
        <Label htmlFor="account-captcha">{t("captcha")}</Label>
        <Button
          type="button"
          variant="link"
          size="sm"
          className="h-6 px-0 text-xs"
          disabled={form.loading || form.remaining > 0}
          onClick={() => void form.sendCode()}
        >
          {form.remaining > 0
            ? t("resendAfter", { seconds: form.remaining })
            : t("sendCode")}
        </Button>
      </div>
      <CaptchaInput
        value={form.values.captcha}
        onChange={(value) => form.setValue("captcha", value)}
        disabled={form.loading}
      />
    </div>
  ) : null
}

export function AccountPasswordField({
  form,
  visible,
  setVisible,
  props,
}: AccountFormScope) {
  const t = useTranslations("AccountFlow")
  const needsPassword =
    props.mode === "password" ||
    props.mode === "register" ||
    props.mode === "reset"
  const password = props.mode === "password"
  return needsPassword ? (
    <div className="grid gap-2">
      <Label htmlFor="account-password">
        {t(password ? "password" : "newPassword")}
      </Label>
      <div className="relative">
        <Input
          id="account-password"
          type={visible ? "text" : "password"}
          autoComplete={password ? "current-password" : "new-password"}
          className="pr-11"
          required
          placeholder={t(password ? "passwordHint" : "newPasswordHint")}
          value={form.values.password}
          onChange={(event) => form.setValue("password", event.target.value)}
          disabled={form.loading}
        />
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="absolute right-1 top-1 size-7 rounded-full text-muted-foreground"
          aria-label={t(visible ? "hidePassword" : "showPassword")}
          onClick={() => setVisible(!visible)}
        >
          {visible ? (
            <EyeOff className="size-3.5" />
          ) : (
            <Eye className="size-3.5" />
          )}
        </Button>
      </div>
    </div>
  ) : null
}

function CaptchaInput({
  value,
  onChange,
  disabled,
}: {
  value: string
  onChange: (value: string) => void
  disabled: boolean
}) {
  return (
    <div className="group relative h-11">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 grid grid-cols-6 gap-2"
      >
        {Array.from({ length: 6 }, (_, index) => (
          <span
            key={index}
            className="flex items-center justify-center rounded-lg border bg-input/20 font-mono text-lg group-focus-within:border-primary/40"
          >
            {value[index] || (
              <span className="h-px w-1.5 bg-muted-foreground/30" />
            )}
          </span>
        ))}
      </div>
      <input
        id="account-captcha"
        autoComplete="one-time-code"
        inputMode="numeric"
        pattern="[0-9]{6}"
        maxLength={6}
        required
        value={value}
        onChange={(event) => onChange(event.target.value.replace(/\D/g, ""))}
        disabled={disabled}
        className="absolute inset-0 w-full rounded-lg bg-transparent text-transparent caret-transparent outline-none focus-visible:ring-2 focus-visible:ring-primary/20"
      />
    </div>
  )
}
