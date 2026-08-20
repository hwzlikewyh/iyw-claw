"use client"

import { useTranslations } from "next-intl"

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  useAddChatChannelDialog,
  type AddChannelDialogState,
  type ParameterChannel,
} from "@/hooks/use-add-chat-channel-dialog"
import type { ChatChannelInfo } from "@/lib/types"
import { ChannelAuthStep } from "./chat-channel-auth-step"
import { ChatChannelQrcodeDialog } from "./chat-channel-qrcode-dialog"
import {
  ChannelFinalizeForm,
  type ChannelFinalizeValues,
} from "./channel-finalize-form"

interface AddChatChannelDialogProps {
  open: boolean
  channelType: ParameterChannel
  draft?: ChatChannelInfo
  onOpenChange: (open: boolean) => void
  onChannelAdded: () => void
}

export function AddChatChannelDialog(props: AddChatChannelDialogProps) {
  const t = useTranslations("ChatChannelSettings")
  const controller = useAddChatChannelDialog({
    ...props,
    labels: {
      draftName: t(`market.draftNames.${props.channelType}`),
      secretRequired: t("secretRequired"),
      larkFieldsRequired: t("market.larkFieldsRequired"),
      botIdRequired: t("botIdRequired"),
      clientIdRequired: t("clientIdRequired"),
      savedButConnectFailed: t("savedButConnectFailed"),
    },
  })
  return (
    <>
      <SetupDialog {...props} {...controller} />
      <QrcodeFlow
        channelType={props.channelType}
        state={controller.state}
        patch={controller.patch}
      />
    </>
  )
}

function SetupDialog({
  open,
  channelType,
  onOpenChange,
  state,
  patch,
  startQr,
  saveCredential,
  finalize,
}: AddChatChannelDialogProps & ReturnType<typeof useAddChatChannelDialog>) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {state.step === 1
              ? t("market.configureTitle", {
                  channel: t(channelTypeLabel(channelType)),
                })
              : t("market.finishSetup")}
          </DialogTitle>
        </DialogHeader>
        {state.step === 1 ? (
          <AuthStep
            channelType={channelType}
            state={state}
            patch={patch}
            startQr={startQr}
            saveCredential={saveCredential}
            onCancel={() => onOpenChange(false)}
          />
        ) : (
          <FinalizeStep
            channelType={channelType}
            state={state}
            onCancel={() => onOpenChange(false)}
            onSubmit={finalize}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function AuthStep({
  channelType,
  state,
  patch,
  startQr,
  saveCredential,
  onCancel,
}: {
  channelType: ParameterChannel
  state: AddChannelDialogState
  patch: ReturnType<typeof useAddChatChannelDialog>["patch"]
  startQr: () => Promise<void>
  saveCredential: () => Promise<void>
  onCancel: () => void
}) {
  return (
    <ChannelAuthStep
      channelType={channelType}
      appId={state.appId}
      botId={state.botId}
      chatId={state.chatId}
      clientId={state.clientId}
      token={state.token}
      larkRegion={state.larkRegion}
      hasToken={state.hasToken}
      loading={state.loading}
      error={state.error}
      onAppIdChange={(appId) => patch({ appId })}
      onBotIdChange={(botId) => patch({ botId })}
      onChatIdChange={(chatId) => patch({ chatId })}
      onClientIdChange={(clientId) => patch({ clientId })}
      onTokenChange={(token) => patch({ token })}
      onLarkRegionChange={(larkRegion) => patch({ larkRegion })}
      onStartQr={() => void startQr()}
      onSaveCredential={() => void saveCredential()}
      onCancel={onCancel}
    />
  )
}

function FinalizeStep({
  channelType,
  state,
  onCancel,
  onSubmit,
}: {
  channelType: ParameterChannel
  state: AddChannelDialogState
  onCancel: () => void
  onSubmit: (values: ChannelFinalizeValues) => Promise<void>
}) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <ChannelFinalizeForm
      channelType={channelType}
      initialName={state.working?.name ?? t(`market.draftNames.${channelType}`)}
      submitting={state.loading}
      error={state.error}
      onCancel={onCancel}
      onSubmit={(values) => void onSubmit(values)}
    />
  )
}

function QrcodeFlow({
  channelType,
  state,
  patch,
}: {
  channelType: ParameterChannel
  state: AddChannelDialogState
  patch: ReturnType<typeof useAddChatChannelDialog>["patch"]
}) {
  if (!state.qrcodeOpen || !state.working) return null
  return (
    <ChatChannelQrcodeDialog
      open
      channelId={state.working.id}
      channelType={channelType}
      variant={channelType === "lark" ? state.larkRegion : undefined}
      onOpenChange={(qrcodeOpen) => patch({ qrcodeOpen })}
      onAuthSuccess={() => patch({ qrcodeOpen: false, step: 2 })}
    />
  )
}

function channelTypeLabel(type: ParameterChannel) {
  return type === "wecom_ai_bot" ? "wecomAiBot" : type
}

export type { ParameterChannel }
