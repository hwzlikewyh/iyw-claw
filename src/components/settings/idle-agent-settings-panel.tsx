"use client"

import { useCallback, useEffect, useState } from "react"
import { Loader2 } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { getIdleAgentSettings, setIdleAgentSettings } from "@/lib/api"

const DEFAULT_MAX_IDLE_AGENTS = 4

function useIdleAgentSettings() {
  const [maxIdle, setMaxIdle] = useState<number | null | undefined>(
    DEFAULT_MAX_IDLE_AGENTS
  )
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let cancelled = false
    void getIdleAgentSettings()
      .then((settings) => {
        if (!cancelled) setMaxIdle(settings.max_idle_connections)
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setError(
            cause instanceof Error ? cause.message : "加载空闲 Agent 设置失败"
          )
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [])
  const save = useCallback(async () => {
    if (maxIdle === undefined || (maxIdle !== null && !isValidLimit(maxIdle))) {
      setError("请输入非负整数")
      return
    }
    setSaving(true)
    try {
      const saved = await setIdleAgentSettings({
        max_idle_connections: maxIdle,
      })
      setMaxIdle(saved.max_idle_connections)
      setError(null)
      toast.success("空闲 Agent 设置已生效")
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "保存空闲 Agent 设置失败"
      setError(message)
      toast.error(message)
    } finally {
      setSaving(false)
    }
  }, [maxIdle])
  return { maxIdle, setMaxIdle, loading, saving, error, save }
}

function isValidLimit(value: number): boolean {
  return Number.isSafeInteger(value) && value >= 0
}

function IdleAgentControls({
  settings,
}: {
  settings: ReturnType<typeof useIdleAgentSettings>
}) {
  const unlimited = settings.maxIdle === null
  return (
    <div className="flex items-center gap-2">
      <label className="flex cursor-pointer select-none items-center gap-2 text-sm text-muted-foreground">
        不限
        <Switch
          checked={unlimited}
          onCheckedChange={(enabled) =>
            settings.setMaxIdle(enabled ? null : DEFAULT_MAX_IDLE_AGENTS)
          }
          disabled={settings.loading || settings.saving}
          aria-label="不限空闲 Agent 数量"
        />
      </label>
      <Input
        id="max-idle-agents"
        type="number"
        min={0}
        step={1}
        inputMode="numeric"
        value={unlimited ? "" : (settings.maxIdle ?? "")}
        onChange={(event) => {
          const raw = event.target.value
          settings.setMaxIdle(raw === "" ? undefined : Number(raw))
        }}
        disabled={unlimited || settings.loading || settings.saving}
        aria-label="空闲 Agent 常驻数量"
        className="w-24"
      />
      <Button
        variant="outline"
        size="sm"
        onClick={settings.save}
        disabled={settings.loading || settings.saving}
      >
        {settings.saving ? (
          <Loader2 className="size-3.5 animate-spin" />
        ) : (
          "保存"
        )}
      </Button>
    </div>
  )
}

export function IdleAgentSettingsPanel() {
  const settings = useIdleAgentSettings()
  return (
    <section className="border-y py-4">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="min-w-0 space-y-1">
          <label htmlFor="max-idle-agents" className="text-sm font-medium">
            空闲 Agent 常驻数量
          </label>
          <p className="text-xs leading-5 text-muted-foreground">
            保留已完成对话的 Agent
            以加快恢复。不限时不按数量回收，系统内存压力仍会强制收缩；正在生成和待发送消息的会话始终受保护。
          </p>
        </div>
        <IdleAgentControls settings={settings} />
      </div>
      {settings.error && (
        <p className="mt-2 text-xs text-destructive">{settings.error}</p>
      )}
    </section>
  )
}
