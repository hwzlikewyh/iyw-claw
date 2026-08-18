"use client"

import { useCallback, useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { agentVersionCenterSnapshot } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import {
  type AgentCapability,
  type CapabilityDecision,
  getAgentCapabilityDecision,
  getClientFileUploadDecision,
  getClientFileUploadPreference,
  listAgentCapabilityPreferences,
  setAgentCapabilityPreference,
  setClientFileUploadPreference,
} from "@/lib/transport/capability-policy"

import {
  CapabilitySection,
  PreferenceRow,
} from "./capability-preference-controls"

const AGENT_CAPABILITIES: AgentCapability[] = [
  "host_execution",
  "host_read",
  "host_write",
  "terminal",
  "mcp",
]

const HOST_EXECUTION_CHILDREN = new Set<AgentCapability>([
  "host_read",
  "host_write",
  "terminal",
])

const EMPTY_AGENT_PREFERENCES: Record<AgentCapability, boolean> = {
  host_execution: false,
  host_read: false,
  host_write: false,
  terminal: false,
  mcp: false,
}

const MAX_SIGNED_PLATFORM_ID = BigInt("9223372036854775807")

function isValidPlatformId(value: string): boolean {
  if (!/^[1-9]\d{0,18}$/.test(value)) return false
  return BigInt(value) <= MAX_SIGNED_PLATFORM_ID
}

export function ClientCapabilityPreferences() {
  const t = useTranslations("AcpAgentSettings.capabilities")
  const [enabled, setEnabled] = useState(false)
  const [mixed, setMixed] = useState(false)
  const [decision, setDecision] = useState<CapabilityDecision | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const preference = await getClientFileUploadPreference()
      setEnabled(preference.enabled)
      setMixed(preference.mixed)
      setDecision(
        preference.enabled ? await getClientFileUploadDecision() : null
      )
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  const onToggle = useCallback(
    async (checked: boolean) => {
      setSaving(true)
      setError(null)
      try {
        const preference = await setClientFileUploadPreference(checked)
        setEnabled(preference.enabled)
        setMixed(preference.mixed)
        setDecision(
          preference.enabled ? await getClientFileUploadDecision() : null
        )
      } catch (reason) {
        setError(toErrorMessage(reason))
        await load()
        toast.error(t("saveFailed"))
      } finally {
        setSaving(false)
      }
    },
    [load, t]
  )

  return (
    <CapabilitySection title={t("clientTitle")} error={error}>
      <PreferenceRow
        id="capability-file-upload"
        label={t("fileUpload")}
        checked={enabled}
        disabled={loading || saving}
        busy={loading || saving}
        mixed={mixed ? t("runtimeMismatch") : null}
        denied={enabled && decision?.enabled === false}
        onCheckedChange={onToggle}
      />
    </CapabilitySection>
  )
}

export function AgentCapabilityPreferences({
  registryId,
}: {
  registryId: string
}) {
  const t = useTranslations("AcpAgentSettings.capabilities")
  const [platformId, setPlatformId] = useState<string | null>(null)
  const [preferences, setPreferences] = useState(EMPTY_AGENT_PREFERENCES)
  const [decisions, setDecisions] = useState<
    Partial<Record<AgentCapability, CapabilityDecision>>
  >({})
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState<AgentCapability | null>(null)
  const [error, setError] = useState<string | null>(null)

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const snapshot = await agentVersionCenterSnapshot()
      const platform = snapshot.catalog.snapshot.platforms.find(
        (item) => item.registryId === registryId
      )
      if (!platform || !isValidPlatformId(platform.id)) {
        setPlatformId(null)
        setPreferences(EMPTY_AGENT_PREFERENCES)
        setDecisions({})
        return
      }
      setPlatformId(platform.id)
      const next = await listAgentCapabilityPreferences(platform.id)
      setPreferences(next)
      setDecisions(await loadEnabledDecisions(platform.id, next))
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      setLoading(false)
    }
  }, [registryId])

  useEffect(() => {
    void load()
  }, [load])

  const onToggle = useCallback(
    async (capability: AgentCapability, checked: boolean) => {
      if (!platformId) return
      setSaving(capability)
      setError(null)
      try {
        await setAgentCapabilityPreference(platformId, capability, checked)
        const next = { ...preferences, [capability]: checked }
        setPreferences(next)
        setDecisions(await loadEnabledDecisions(platformId, next))
      } catch (reason) {
        setError(toErrorMessage(reason))
        await load()
        toast.error(t("saveFailed"))
      } finally {
        setSaving(null)
      }
    },
    [load, platformId, preferences, t]
  )

  return (
    <CapabilitySection title={t("agentTitle")} error={error}>
      {!loading && !platformId ? (
        <p className="text-xs text-muted-foreground">
          {t("platformUnavailable")}
        </p>
      ) : (
        AGENT_CAPABILITIES.map((capability) => {
          const isChild = HOST_EXECUTION_CHILDREN.has(capability)
          const disabled =
            loading ||
            saving !== null ||
            (isChild && !preferences.host_execution)
          return (
            <PreferenceRow
              key={capability}
              id={`capability-${capability}`}
              label={t(capability)}
              checked={preferences[capability]}
              disabled={disabled}
              busy={loading || saving === capability}
              denied={
                preferences[capability] &&
                (!isChild || preferences.host_execution) &&
                decisions[capability]?.enabled === false
              }
              onCheckedChange={(checked) => void onToggle(capability, checked)}
            />
          )
        })
      )}
    </CapabilitySection>
  )
}

async function loadEnabledDecisions(
  platformId: string,
  preferences: Record<AgentCapability, boolean>
) {
  const enabled = AGENT_CAPABILITIES.filter(
    (capability) => preferences[capability]
  )
  const entries = await Promise.all(
    enabled.map(
      async (capability) =>
        [
          capability,
          await getAgentCapabilityDecision(platformId, capability),
        ] as const
    )
  )
  return Object.fromEntries(entries) as Partial<
    Record<AgentCapability, CapabilityDecision>
  >
}
