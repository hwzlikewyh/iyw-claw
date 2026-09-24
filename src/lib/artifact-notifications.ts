import { getTransport } from "@/lib/transport"

export type ArtifactNotificationMode = "off" | "auto" | "always"

export interface ArtifactChannelTarget {
  channelId: number
  name: string
  notificationMode: ArtifactNotificationMode
  targetId: string | null
  targetName: string | null
  error: string | null
}

export interface ArtifactDeliveryResult {
  status: string
  delivery_unknown?: boolean
  channel_name?: string
  target_name?: string
  error?: string
  reason?: string
  message_error?: string
  file_error?: string
  artifact_errors?: { error: string }[]
  items?: ArtifactDeliveryResult[]
}

export function getArtifactNotificationMode(
  channelId: number
): Promise<ArtifactNotificationMode> {
  return getTransport().call("get_artifact_notification_mode", { channelId })
}

export function setArtifactNotificationMode(
  channelId: number,
  mode: ArtifactNotificationMode
) {
  return getTransport().call<void>("set_artifact_notification_mode", {
    channelId,
    mode,
  })
}

export function listArtifactChannelTargets(): Promise<ArtifactChannelTarget[]> {
  return getTransport().call("list_artifact_channel_targets")
}

const SEND_TIMEOUT_MS = 5 * 60 * 1000
const pendingSends = new WeakMap<
  ReturnType<typeof getTransport>,
  Map<
    string,
    {
      id: string
      promise?: Promise<ArtifactDeliveryResult>
    }
  >
>()

export function sendArtifactToChannels(params: {
  artifactId: number
  conversationId: number
  channelId?: number
  requestId: string
}): Promise<ArtifactDeliveryResult> {
  const transport = getTransport()
  const requests = pendingSends.get(transport) ?? new Map()
  pendingSends.set(transport, requests)
  const key = `${params.conversationId}:${params.artifactId}:${params.channelId ?? "all"}`
  const request = requests.get(key) ?? { id: params.requestId }
  if (request.promise) return request.promise
  requests.set(key, request)
  request.promise = transport
    .call<ArtifactDeliveryResult>(
      "send_artifact_to_channels",
      { params: { ...params, requestId: request.id } },
      { timeoutMs: SEND_TIMEOUT_MS }
    )
    .then((result) => {
      if (
        !result.delivery_unknown &&
        result.status !== "processing" &&
        result.status !== "unknown"
      )
        requests.delete(key)
      return result
    })
    .finally(() => {
      request.promise = undefined
    })
  return request.promise
}
