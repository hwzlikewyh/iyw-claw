import {
  createChatChannel,
  deleteChatChannel,
  listChatChannels,
  testChatChannel,
  updateChatChannel,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { generateWecomAgentSecrets } from "@/lib/chat-channel-setup"
import type { ChannelFinalizeValues } from "./channel-finalize-form"
import { saveWecomAgentSecrets } from "./wecom-agent-setup-api"
import { normalizeHttpsUrl } from "./wecom-agent-setup-parts"
import type { WecomAgentSetupState } from "./wecom-agent-setup-state"

export interface SetupMessages {
  parametersRequired: string
  draftName: string
  reportUserRequired: string
  savedButConnectFailed: string
}

export interface SetupCallbacks {
  onOpenChange: (open: boolean) => void
  onComplete: () => void
}

export async function saveWecomParameters(
  state: WecomAgentSetupState,
  messages: SetupMessages
) {
  const normalizedUrl = normalizeHttpsUrl(state.externalUrl)
  if (!validParameters(state, normalizedUrl)) {
    state.setError(messages.parametersRequired)
    return
  }
  state.setLoading(true)
  state.setError(null)
  try {
    const channel = await upsertDraft(state, normalizedUrl!, messages.draftName)
    state.setWorking(channel)
    await testChatChannel(channel.id)
    state.setSecretsAvailable(true)
    state.setCallbackVerified(false)
    state.setStep(3)
  } catch (error) {
    state.setError(toErrorMessage(error))
  } finally {
    state.setLoading(false)
  }
}

export async function regenerateWecomSecrets(
  state: WecomAgentSetupState,
  next: ReturnType<typeof generateWecomAgentSecrets>
) {
  if (
    !state.working ||
    !state.appSecret.trim() ||
    state.regenerationInFlight.current
  ) {
    return
  }
  state.regenerationInFlight.current = true
  state.setLoading(true)
  state.setError(null)
  try {
    await saveWecomAgentSecrets(
      state.working.id,
      state.appSecret,
      next,
      JSON.stringify({
        callbackPath: next.callbackPath,
        setupState: "pending_callback",
      })
    )
    const updated = await findChannel(state.working.id)
    state.setWorking(updated)
    state.setSecrets(next)
    state.setSecretsAvailable(true)
    state.setCallbackVerified(false)
  } catch (error) {
    state.setError(toErrorMessage(error))
  } finally {
    state.setLoading(false)
    state.regenerationInFlight.current = false
  }
}

export function confirmWecomRegeneration(state: WecomAgentSetupState) {
  const next = generateWecomAgentSecrets()
  state.setRegenerateOpen(false)
  state.setCallbackVerified(false)
  if (!state.appSecret.trim()) {
    state.setSecrets(next)
    state.setSecretsAvailable(true)
    state.setStep(2)
    return
  }
  void regenerateWecomSecrets(state, next)
}

export async function finalizeWecomSetup(
  state: WecomAgentSetupState,
  input: {
    values: ChannelFinalizeValues
    messages: SetupMessages
    callbacks: SetupCallbacks
  }
) {
  if (!state.working) return
  if (input.values.dailyReportEnabled && !input.values.defaultUserId) {
    state.setError(input.messages.reportUserRequired)
    return
  }
  state.setLoading(true)
  state.setError(null)
  try {
    const updated = await updateChatChannel(
      finalizePatch(state.working.id, input.values)
    )
    if (updated.runtime_status === "error") {
      state.setError(updated.last_error ?? input.messages.savedButConnectFailed)
      return
    }
    input.callbacks.onOpenChange(false)
    input.callbacks.onComplete()
  } catch (error) {
    state.setError(toErrorMessage(error))
  } finally {
    state.setLoading(false)
  }
}

function validParameters(
  state: WecomAgentSetupState,
  normalizedUrl: string | null
) {
  return Boolean(
    normalizedUrl &&
    state.corpId.trim() &&
    /^\d+$/.test(state.agentId) &&
    state.appSecret.trim()
  )
}

async function upsertDraft(
  state: WecomAgentSetupState,
  externalBaseUrl: string,
  draftName: string
) {
  const config = {
    corp_id: state.corpId.trim(),
    agent_id: state.agentId.trim(),
    callback_path: state.secrets.callbackPath,
    external_base_url: externalBaseUrl,
    setup_state: "pending_callback",
  }
  if (state.working) {
    await saveWecomAgentSecrets(
      state.working.id,
      state.appSecret,
      state.secrets,
      JSON.stringify({
        corpId: config.corp_id,
        agentId: config.agent_id,
        callbackPath: config.callback_path,
        externalBaseUrl: config.external_base_url,
        setupState: config.setup_state,
      })
    )
    return findChannel(state.working.id)
  }
  const channel = await createChatChannel({
    name: draftName,
    channelType: "wecom_agent",
    configJson: JSON.stringify(config),
    enabled: false,
    dailyReportEnabled: false,
  })
  try {
    await saveWecomAgentSecrets(channel.id, state.appSecret, state.secrets)
    return channel
  } catch (error) {
    await deleteChatChannel(channel.id).catch(() => undefined)
    throw error
  }
}

async function findChannel(channelId: number) {
  const channel = (await listChatChannels()).find(({ id }) => id === channelId)
  if (!channel)
    throw new Error(`Chat channel ${channelId} not found after save`)
  return channel
}

function finalizePatch(channelId: number, values: ChannelFinalizeValues) {
  return {
    id: channelId,
    name: values.name,
    enabled: true,
    configPatchJson: JSON.stringify({
      setupState: "ready",
      defaultAgentType: values.defaultAgentType,
      defaultUserId: values.defaultUserId || null,
    }),
    dailyReportEnabled: values.dailyReportEnabled,
    dailyReportTime: values.dailyReportEnabled ? values.dailyReportTime : null,
  }
}
