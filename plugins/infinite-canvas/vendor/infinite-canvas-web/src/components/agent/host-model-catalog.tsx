import { useEffect, useState } from "react"
import { Alert, Button } from "antd"
import { useTranslation } from "react-i18next"
import { encodeChannelModel, isIywGatewayBaseUrl, modelOptionsFromChannels, useConfigStore, type ChannelModel, type ModelCapability } from "@/stores/use-config-store"
import { hostModelsReady, setHostModelsReady } from "@/lib/host-model-state"
import type {} from "../../../../../widget/src/upstream/host-bridge"

let loading: Promise<ChannelModel[]> | undefined

async function loadModels(): Promise<ChannelModel[]> {
  const response = await fetch("http://iyw-canvas.local/v1/models?model_type=all")
  if (!response.ok) throw new Error("hostAgent.modelsFailed")
  const payload = await response.json() as { data?: unknown }
  if (!Array.isArray(payload.data)) throw new Error("hostAgent.modelsFailed")
  const models = new Map<string, ChannelModel>()
  for (const entry of payload.data) {
    if (!entry || typeof entry.id !== "string" || !entry.id.trim()) continue
    const capability = modelCapability(entry)
    if (capability) models.set(entry.id, { name: entry.id, displayName: typeof entry.display_name === "string" && entry.display_name.trim() ? entry.display_name : entry.id, capability })
  }
  return [...models.values()]
}

function modelCapability(entry: Record<string, unknown>): ModelCapability | undefined {
  const value = entry.capabilities as Record<string, unknown> | undefined
  if (!value || value.embedding === true || value.rerank === true) return undefined
  if (value.image_generation === true || value.image_editing === true) return "image"
  if (value.video_generation === true) return "video"
  if (value.audio_generation === true || value.audio_output === true) return "audio"
  return "text"
}

function applyModels(models: ChannelModel[]): void {
  const current = useConfigStore.getState().config
  const channels = current.channels.map((channel) => isIywGatewayBaseUrl(channel.baseUrl) ? { ...channel, name: "原助理", models } : channel)
  const host = channels.find((channel) => isIywGatewayBaseUrl(channel.baseUrl))
  if (!host) return
  const first = (capability: ModelCapability) => {
    const model = models.find((model) => model.capability === capability)
    return model ? encodeChannelModel(host.id, model.name) : ""
  }
  const bridge = window.__IYW_CANVAS_BRIDGE__
  if (bridge) bridge.modelNames = Object.fromEntries(models.map((model) => [model.name, model.displayName || model.name]))
  setHostModelsReady(true)
  useConfigStore.setState({ config: { ...current, channels, models: modelOptionsFromChannels(channels),
    model: current.model || first("image"), imageModel: current.imageModel || first("image"), textModel: current.textModel || first("text"),
    videoModel: current.videoModel || first("video"), audioModel: current.audioModel || first("audio"),
  } })
}

export function HostModelCatalog() {
  const { t } = useTranslation()
  const [attempt, setAttempt] = useState(0)
  const [status, setStatus] = useState("loading")
  useEffect(() => {
    let active = true
    setStatus("loading")
    if (hostModelsReady()) {
      setHostModelsReady(false)
      useConfigStore.setState(({ config }) => ({ config: { ...config } }))
    }
    loading ??= loadModels().finally(() => { loading = undefined })
    void loading.then((models) => {
      if (!active) return
      applyModels(models); setStatus(models.length ? "ready" : "empty")
    }, () => { if (active) setStatus("failed") })
    return () => { active = false }
  }, [attempt])
  useEffect(() => {
    const refresh = () => setAttempt((value) => value + 1)
    window.addEventListener("iyw-canvas-refresh-models", refresh)
    return () => window.removeEventListener("iyw-canvas-refresh-models", refresh)
  }, [])
  if (status === "ready") return null
  return <Alert banner type={status === "failed" ? "error" : "info"} title={t(status === "loading" ? "hostAgent.modelsLoading" : status === "empty" ? "hostAgent.modelsEmpty" : "hostAgent.modelsFailed")} action={status !== "loading" ? <Button size="small" onClick={() => setAttempt((value) => value + 1)}>{t("hostAgent.modelsRefresh")}</Button> : undefined} />
}
