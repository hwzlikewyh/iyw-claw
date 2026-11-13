import { useRef, useState } from "react"
import { Alert, Button, Drawer, Input } from "antd"
import { useTranslation } from "react-i18next"
import { useAgentStore } from "@/stores/use-agent-store"
import { useHostAgentConnection } from "./use-host-agent-connection"
import { sendHostCanvasTask } from "./host-agent-send"
import "./host-agent-text"

export function HostAgentPanel() {
  const { t } = useTranslation()
  const { reconnect, currentWindow } = useHostAgentConnection()
  const open = useAgentStore((state) => state.panelOpen)
  const close = useAgentStore((state) => state.closePanel)
  const connected = useAgentStore((state) => state.connected)
  const connectionError = useAgentStore((state) => state.connectError)
  const snapshot = useAgentStore((state) => state.canvasContext?.snapshot)
  const [prompt, setPrompt] = useState("")
  const [sending, setSending] = useState(false)
  const [result, setResult] = useState<{ error?: string; sent?: boolean }>({})
  const pending = useRef(false)
  const send = async () => {
    if (pending.current || !connected || !snapshot || !prompt.trim()) return
    pending.current = true; setSending(true); setResult({})
    try { await sendHostCanvasTask(prompt); setPrompt(""); setResult({ sent: true }) }
    catch (error) { setResult({ error: error instanceof Error ? error.message : t("hostAgent.failed") }) }
    finally { pending.current = false; setSending(false) }
  }
  return <Drawer title={t("hostAgent.name")} open={open} onClose={close} size={400} styles={{ wrapper: { maxWidth: "100vw" } }}>
    <div className="flex flex-col gap-5">
      <p className="text-sm text-stone-600 dark:text-stone-400">{t("hostAgent.description")}</p>
      {currentWindow ? <p className="text-sm">{t("hostAgent.window", { agent: currentWindow.agentLabel, model: currentWindow.modelName || t("hostAgent.noModel") })}</p> : null}
      <p className="text-sm" role="status">{snapshot ? t("hostAgent.context", { title: snapshot.title, count: snapshot.selectedNodeIds.length }) : t("hostAgent.noCanvas")}</p>
      {connectionError ? <Alert type="error" title={connectionError} action={<Button size="small" onClick={reconnect}>{t("hostAgent.retry")}</Button>} /> : null}
      <label htmlFor="host-agent-prompt" className="text-sm font-medium">{t("hostAgent.prompt")}</label>
      <Input.TextArea id="host-agent-prompt" value={prompt} onChange={(event) => setPrompt(event.target.value)} placeholder={t("hostAgent.placeholder")} autoSize={{ minRows: 5, maxRows: 12 }} disabled={sending} />
      <Button type="primary" disabled={!connected || !snapshot || !prompt.trim()} loading={sending} onClick={() => void send()}>{t(sending ? "hostAgent.sending" : "hostAgent.send")}</Button>
      {result.error ? <Alert type="error" title={result.error} /> : null}
      {result.sent ? <Alert type="success" title={t("hostAgent.sent")} /> : null}
    </div>
  </Drawer>
}
