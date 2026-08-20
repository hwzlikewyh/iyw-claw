"use client"

import { useCallback, useEffect, useState } from "react"

import {
  createChatChannel,
  getChatChannelHasToken,
  saveChatChannelToken,
  testChatChannel,
  updateChatChannel,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { parseChannelConfig } from "@/lib/chat-channel-setup"
import type { ChatChannelInfo } from "@/lib/types"
import type { ChannelFinalizeValues } from "@/components/settings/channel-finalize-form"

export type ParameterChannel = "lark" | "wecom_ai_bot" | "dingtalk"

export interface AddChannelDialogState {
  step: 1 | 2
  working?: ChatChannelInfo
  token: string
  appId: string
  botId: string
  clientId: string
  larkRegion: "feishu" | "lark"
  chatId: string
  hasToken: boolean
  loading: boolean
  qrcodeOpen: boolean
  error: string | null
}

type StatePatch = (patch: Partial<AddChannelDialogState>) => void

interface DialogLabels {
  draftName: string
  secretRequired: string
  larkFieldsRequired: string
  botIdRequired: string
  clientIdRequired: string
  savedButConnectFailed: string
}

interface DialogControllerParams {
  open: boolean
  channelType: ParameterChannel
  draft?: ChatChannelInfo
  labels: DialogLabels
  onOpenChange: (open: boolean) => void
  onChannelAdded: () => void
}

interface ActionContext extends DialogControllerParams {
  state: AddChannelDialogState
  patch: StatePatch
}

export function useAddChatChannelDialog(params: DialogControllerParams) {
  const [state, setState] = useState(() => initialState(params.draft))
  const patch = useCallback<StatePatch>(
    (next) => setState((current) => ({ ...current, ...next })),
    []
  )
  useSavedToken(params.open, params.draft, patch)
  const context = { ...params, state, patch }
  return {
    state,
    patch,
    startQr: () => startQr(context),
    saveCredential: () => saveCredential(context),
    finalize: (values: ChannelFinalizeValues) => finalize(context, values),
  }
}

function initialState(draft?: ChatChannelInfo): AddChannelDialogState {
  const stored = draft ? parseChannelConfig(draft) : {}
  return {
    step: 1,
    working: draft,
    token: "",
    appId: stored.app_id ?? "",
    botId: stored.bot_id ?? "",
    clientId: stored.client_id ?? "",
    larkRegion: stored.lark_region === "lark" ? "lark" : "feishu",
    chatId: stored.chat_id ?? stored.default_chatid ?? "",
    hasToken: false,
    loading: false,
    qrcodeOpen: false,
    error: null,
  }
}

function useSavedToken(
  open: boolean,
  draft: ChatChannelInfo | undefined,
  patch: StatePatch
) {
  useEffect(() => {
    if (!open || !draft) return
    getChatChannelHasToken(draft.id)
      .then((hasToken) => patch({ hasToken }))
      .catch(() => {})
  }, [draft, open, patch])
}

async function startQr(context: ActionContext) {
  await runAction(context.patch, async () => {
    const working = await ensureQrDraft(context)
    context.patch({ working, qrcodeOpen: true })
  })
}

async function ensureQrDraft(context: ActionContext) {
  if (!context.state.working) {
    return createChatChannel({
      name: context.labels.draftName,
      channelType: context.channelType,
      configJson: buildDraftConfig(context.channelType, context.state),
      enabled: false,
      dailyReportEnabled: false,
    })
  }
  if (context.channelType !== "lark") return context.state.working
  return updateChatChannel({
    id: context.state.working.id,
    configPatchJson: JSON.stringify({
      larkRegion: context.state.larkRegion,
    }),
  })
}

async function saveCredential(context: ActionContext) {
  const error = validationError(context)
  if (error) {
    context.patch({ error })
    return
  }
  await runAction(context.patch, async () => {
    const working = await saveManualChannel(context)
    if (context.state.token.trim()) {
      await saveChatChannelToken(working.id, context.state.token.trim())
    }
    context.patch({ working })
    await testChatChannel(working.id)
    context.patch({ step: 2 })
  })
}

function validationError(context: ActionContext) {
  const { channelType, labels, state } = context
  if (!state.token.trim() && !state.hasToken) return labels.secretRequired
  if (channelType === "lark" && (!state.appId.trim() || !state.chatId.trim())) {
    return labels.larkFieldsRequired
  }
  if (channelType === "wecom_ai_bot" && !state.botId.trim()) {
    return labels.botIdRequired
  }
  if (channelType === "dingtalk" && !state.clientId.trim()) {
    return labels.clientIdRequired
  }
  return null
}

async function saveManualChannel(context: ActionContext) {
  if (!context.state.working) {
    return createChatChannel({
      name: context.labels.draftName,
      channelType: context.channelType,
      configJson: buildDraftConfig(context.channelType, context.state),
      enabled: false,
      dailyReportEnabled: false,
    })
  }
  return updateChatChannel({
    id: context.state.working.id,
    configPatchJson: JSON.stringify({
      ...manualConfigPatch(context.channelType, context.state),
      setupState: "pending_auth",
    }),
  })
}

function buildDraftConfig(
  channelType: ParameterChannel,
  state: AddChannelDialogState
) {
  const base = { setup_state: "pending_auth" }
  if (channelType === "lark") {
    return JSON.stringify({
      ...base,
      app_id: state.appId.trim(),
      chat_id: state.chatId.trim(),
      lark_region: state.larkRegion,
    })
  }
  if (channelType === "wecom_ai_bot") {
    return JSON.stringify({
      ...base,
      bot_id: state.botId.trim(),
      default_chatid: state.chatId.trim(),
    })
  }
  return JSON.stringify({ ...base, client_id: state.clientId.trim() })
}

function manualConfigPatch(
  channelType: ParameterChannel,
  state: AddChannelDialogState
) {
  if (channelType === "lark") {
    return {
      appId: state.appId.trim(),
      chatId: state.chatId.trim(),
      larkRegion: state.larkRegion,
    }
  }
  if (channelType === "wecom_ai_bot") {
    return { botId: state.botId.trim(), defaultChatid: state.chatId.trim() }
  }
  return { clientId: state.clientId.trim() }
}

async function finalize(context: ActionContext, values: ChannelFinalizeValues) {
  if (!context.state.working) return
  await runAction(context.patch, async () => {
    const updated = await updateChatChannel({
      id: context.state.working!.id,
      name: values.name,
      enabled: true,
      configPatchJson: JSON.stringify({
        setupState: "ready",
        defaultAgentType: values.defaultAgentType,
      }),
      dailyReportEnabled:
        context.channelType !== "dingtalk" && values.dailyReportEnabled,
      dailyReportTime:
        context.channelType !== "dingtalk" && values.dailyReportEnabled
          ? values.dailyReportTime
          : null,
    })
    if (updated.runtime_status === "error") {
      context.patch({
        error: updated.last_error ?? context.labels.savedButConnectFailed,
      })
      return
    }
    context.onOpenChange(false)
    context.onChannelAdded()
  })
}

async function runAction(patch: StatePatch, action: () => Promise<void>) {
  patch({ loading: true, error: null })
  try {
    await action()
  } catch (error) {
    patch({ error: toErrorMessage(error) })
  } finally {
    patch({ loading: false })
  }
}
