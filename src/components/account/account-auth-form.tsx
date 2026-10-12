"use client"

import { useState } from "react"
import { AccountAgreement, AccountFlowError } from "./account-flow-feedback"
import { AccountImageCaptcha } from "./account-image-captcha"
import {
  useAccountLoginForm,
  type AccountFormMode,
} from "./use-account-login-form"
import {
  AccountCaptchaField,
  AccountIdentityField,
  AccountPasswordField,
} from "./account-auth-fields"
import {
  AccountFormBack,
  AccountFormLinks,
  AccountRescanButton,
  AccountSecureCaption,
  AccountSubmitButton,
  BindingProgress,
} from "./account-auth-actions"

interface Props {
  mode: AccountFormMode
  onMode: (mode: "wechat" | AccountFormMode) => void
  onDone: () => void
}

export interface AccountFormScope {
  props: Props
  form: ReturnType<typeof useAccountLoginForm>
  visible: boolean
  setVisible: (visible: boolean) => void
}

export function AccountAuthForm(props: Props) {
  const form = useAccountLoginForm(props.mode, props.onDone)
  const [visible, setVisible] = useState(false)
  const scope = { props, form, visible, setVisible }
  return (
    <form
      className="mx-auto grid w-full max-w-sm gap-5"
      onSubmit={(event) => {
        event.preventDefault()
        void form.submit()
      }}
    >
      <BindingProgress {...scope} />
      <AccountFormBack {...scope} />
      <AccountIdentityField {...scope} />
      <AccountCaptchaField {...scope} />
      <AccountPasswordField {...scope} />
      <AccountFormLinks {...scope} />
      {form.imageChallenge ? (
        <AccountImageCaptcha
          challenge={form.imageChallenge}
          loading={form.loading}
          onVerify={form.verifyImage}
          onRefresh={form.refreshImage}
          onCancel={form.cancelImage}
        />
      ) : null}
      <AccountFlowError message={form.error} />
      <AccountSubmitButton {...scope} />
      <AccountRescanButton {...scope} />
      <AccountAgreement />
      <AccountSecureCaption {...scope} />
    </form>
  )
}
