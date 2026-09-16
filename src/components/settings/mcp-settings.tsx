"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  Globe,
  Loader2,
  Plus,
  RefreshCw,
  Search,
  ShieldCheck,
  TerminalSquare,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { MarketItemIcon } from "@/components/skills/market/market-item-icon"
import {
  ConnectorDetailMetadata,
  ConnectorListMetadata,
  connectorName,
  pluginSources,
} from "@/components/settings/connector-metadata"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import {
  mcpGetMarketplaceServerDetail,
  mcpInstallFromMarketplace,
  mcpListMarketplaces,
  mcpRemoveServer,
  mcpScanLocal,
  mcpSearchMarketplace,
  mcpSetServerEnabled,
  mcpUpsertLocalServer,
} from "@/lib/api"
import { toLocalizedErrorMessage } from "@/lib/app-error"
import { normalizeMcpType } from "@/lib/mcp-types"
import { cn } from "@/lib/utils"
import type {
  LocalMcpServer,
  McpMarketplaceItem,
  McpMarketplaceInstallOption,
  McpMarketplaceProvider,
  McpMarketplaceServerDetail,
} from "@/lib/types"

type LeftTab = "local" | "market" | "custom"

type Selection =
  | { kind: "local"; id: string }
  | { kind: "market"; id: string }
  | { kind: "draft" }
  | null

const DEFAULT_DRAFT_SPEC = JSON.stringify(
  {
    type: "stdio",
    command: "",
    args: [],
  },
  null,
  2
)

type McpTranslator = (
  key: string,
  values?: Record<string, string | number>
) => string

function isObject(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value)
}

function readString(spec: Record<string, unknown>, key: string): string | null {
  const raw = spec[key]
  if (typeof raw !== "string") return null
  const trimmed = raw.trim()
  return trimmed ? trimmed : null
}

function specSummary(spec: Record<string, unknown>, t: McpTranslator): string {
  const typ = readString(spec, "type") ?? "stdio"

  if (typ === "stdio") {
    const command = readString(spec, "command") ?? t("summary.missingCommand")
    const rawArgs = spec.args
    const args = Array.isArray(rawArgs)
      ? rawArgs
          .map((item) => (typeof item === "string" ? item.trim() : ""))
          .filter(Boolean)
      : []
    return args.length > 0 ? `${command} ${args.join(" ")}` : command
  }

  const url = readString(spec, "url") ?? t("summary.missingUrl")
  return `${typ}: ${url}`
}

function protocolBadgeLabel(protocol: string, t: McpTranslator): string {
  const canonical = normalizeMcpType(protocol)
  if (canonical === "stdio") return t("protocol.stdio")
  if (canonical === "sse") return "SSE"
  if (canonical === "http") return "HTTP"
  return protocol
}

function defaultParamDraft(
  option: McpMarketplaceInstallOption | null
): Record<string, string> {
  if (!option) return {}
  const draft: Record<string, string> = {}
  for (const field of option.parameters) {
    if (field.default_value === null || field.default_value === undefined)
      continue
    if (typeof field.default_value === "string") {
      draft[field.key] = field.default_value
      continue
    }
    if (
      typeof field.default_value === "number" ||
      typeof field.default_value === "boolean"
    ) {
      draft[field.key] = String(field.default_value)
      continue
    }
    draft[field.key] = JSON.stringify(field.default_value)
  }
  return draft
}

function parseParameterValues(
  option: McpMarketplaceInstallOption | null,
  draft: Record<string, string>,
  t: McpTranslator
): { values: Record<string, unknown>; error: string | null } {
  if (!option) return { values: {}, error: t("errors.selectInstallProtocol") }

  const values: Record<string, unknown> = {}
  for (const field of option.parameters) {
    const raw = (draft[field.key] ?? "").trim()

    if (!raw) {
      if (field.required && field.default_value == null) {
        return {
          values: {},
          error: t("errors.fieldRequired", { field: field.label }),
        }
      }
      continue
    }

    if (field.kind === "boolean") {
      if (raw !== "true" && raw !== "false") {
        return {
          values: {},
          error: t("errors.fieldNeedsBoolean", { field: field.label }),
        }
      }
      values[field.key] = raw === "true"
      continue
    }

    if (field.kind === "number") {
      const next = Number(raw)
      if (!Number.isFinite(next)) {
        return {
          values: {},
          error: t("errors.fieldNeedsNumber", { field: field.label }),
        }
      }
      values[field.key] = next
      continue
    }

    if (field.kind === "integer") {
      const next = Number(raw)
      if (!Number.isInteger(next)) {
        return {
          values: {},
          error: t("errors.fieldNeedsInteger", { field: field.label }),
        }
      }
      values[field.key] = next
      continue
    }

    if (field.kind === "json") {
      try {
        values[field.key] = JSON.parse(raw)
      } catch (err) {
        const message = toLocalizedErrorMessage(err, t)
        return {
          values: {},
          error: t("errors.fieldInvalidJson", {
            field: field.label,
            message,
          }),
        }
      }
      continue
    }

    if (field.enum_values.length > 0 && !field.enum_values.includes(raw)) {
      return {
        values: {},
        error: t("errors.fieldOutOfRange", { field: field.label }),
      }
    }

    values[field.key] = raw
  }

  return { values, error: null }
}

function detectEnvOnRemote(text: string): boolean {
  const trimmed = text.trim()
  if (!trimmed) return false
  let parsed: unknown
  try {
    parsed = JSON.parse(trimmed)
  } catch {
    return false
  }
  if (!isObject(parsed)) return false

  const rawType = typeof parsed.type === "string" ? parsed.type : ""
  const canonical = normalizeMcpType(rawType)
  if (canonical !== "http" && canonical !== "sse") return false

  const env = parsed.env
  if (!isObject(env)) return false
  return Object.keys(env).length > 0
}

function parseJsonObject(
  text: string,
  name: string,
  t: McpTranslator
): Record<string, unknown> {
  const trimmed = text.trim()
  if (!trimmed) {
    throw new Error(t("errors.jsonEmpty", { name }))
  }

  let parsed: unknown
  try {
    parsed = JSON.parse(trimmed)
  } catch (err) {
    const message = toLocalizedErrorMessage(err, t)
    throw new Error(t("errors.jsonInvalid", { name, message }))
  }

  if (!isObject(parsed)) {
    throw new Error(t("errors.jsonMustBeObject", { name }))
  }

  return parsed
}

export function McpSettings({ embedded = false }: { embedded?: boolean }) {
  const t = useTranslations("McpSettings")
  const mcpT = useMemo(() => t as unknown as McpTranslator, [t])
  const [loading, setLoading] = useState(true)
  const [loadingError, setLoadingError] = useState<string | null>(null)

  const [leftTab, setLeftTab] = useState<LeftTab>("local")
  const [selection, setSelection] = useState<Selection>(null)

  const [installedServers, setInstalledServers] = useState<LocalMcpServer[]>([])
  const [localFilter, setLocalFilter] = useState("")

  const [providers, setProviders] = useState<McpMarketplaceProvider[]>([])
  const [selectedProvider, setSelectedProvider] = useState("")
  const [marketQuery, setMarketQuery] = useState("")
  const marketQueryRef = useRef("")
  const marketSearchRequestRef = useRef(0)
  const [searching, setSearching] = useState(false)
  const [searchError, setSearchError] = useState<string | null>(null)
  const [searchResults, setSearchResults] = useState<McpMarketplaceItem[]>([])

  const [marketDetail, setMarketDetail] =
    useState<McpMarketplaceServerDetail | null>(null)
  const [marketDetailLoading, setMarketDetailLoading] = useState(false)
  const [marketDetailError, setMarketDetailError] = useState<string | null>(
    null
  )
  const [marketSpecText, setMarketSpecText] = useState("")
  const [marketSpecDirty, setMarketSpecDirty] = useState(false)
  const [selectedInstallOptionId, setSelectedInstallOptionId] = useState("")
  const [installParamDraft, setInstallParamDraft] = useState<
    Record<string, string>
  >({})

  const [localSpecText, setLocalSpecText] = useState("")

  const [installDialogOpen, setInstallDialogOpen] = useState(false)

  const [draftServerId, setDraftServerId] = useState("")
  const [draftSpecText, setDraftSpecText] = useState(DEFAULT_DRAFT_SPEC)

  const [runningAction, setRunningAction] = useState<string | null>(null)

  const selectedLocal = useMemo(() => {
    if (selection?.kind !== "local") return null
    return installedServers.find((item) => item.id === selection.id) ?? null
  }, [installedServers, selection])

  const selectedMarketItem = useMemo(() => {
    if (selection?.kind !== "market") return null
    return searchResults.find((item) => item.server_id === selection.id) ?? null
  }, [searchResults, selection])

  const selectedInstallOption = useMemo(() => {
    if (!marketDetail) return null
    return (
      marketDetail.install_options.find(
        (item) => item.id === selectedInstallOptionId
      ) ??
      marketDetail.install_options[0] ??
      null
    )
  }, [marketDetail, selectedInstallOptionId])

  const draftEnvOnRemote = useMemo(
    () => detectEnvOnRemote(draftSpecText),
    [draftSpecText]
  )
  const localEnvOnRemote = useMemo(
    () => detectEnvOnRemote(localSpecText),
    [localSpecText]
  )

  const filteredLocalServers = useMemo(() => {
    const q = localFilter.trim().toLowerCase()
    if (!q) return installedServers
    return installedServers.filter((item) => {
      if (item.id.toLowerCase().includes(q)) return true
      if (connectorName(item).toLowerCase().includes(q)) return true
      if (
        pluginSources(item).some((source) =>
          source.ownerName.toLowerCase().includes(q)
        )
      ) {
        return true
      }
      const spec = isObject(item.spec) ? item.spec : {}
      return specSummary(spec, mcpT).toLowerCase().includes(q)
    })
  }, [installedServers, localFilter, mcpT])

  const refreshLocalServers = useCallback(async () => {
    const servers = await mcpScanLocal()
    setInstalledServers(servers)
    return servers
  }, [])

  const loadInitial = useCallback(async () => {
    setLoading(true)
    setLoadingError(null)

    try {
      const [servers, marketProviders] = await Promise.all([
        mcpScanLocal(),
        mcpListMarketplaces(),
      ])
      setInstalledServers(servers)
      setProviders(marketProviders)
      setSelectedProvider(
        (current) => current || marketProviders[0]?.id || "official_registry"
      )

      if (servers[0]) {
        setSelection({ kind: "local", id: servers[0].id })
      }
    } catch (err) {
      const message = toLocalizedErrorMessage(err, mcpT)
      setLoadingError(message)
    } finally {
      setLoading(false)
    }
  }, [mcpT])

  useEffect(() => {
    loadInitial().catch((err) => {
      console.error("[Settings] load MCP settings failed:", err)
    })
  }, [loadInitial])

  useEffect(() => {
    if (!selectedLocal) return
    const nextSpec = JSON.stringify(selectedLocal.spec, null, 2)
    setLocalSpecText(nextSpec)
  }, [selectedLocal])

  useEffect(() => {
    if (selection?.kind !== "market" || !selectedMarketItem) {
      setMarketDetail(null)
      setMarketDetailError(null)
      setMarketSpecText("")
      setMarketSpecDirty(false)
      setSelectedInstallOptionId("")
      setInstallParamDraft({})
      return
    }

    let cancelled = false
    setMarketDetailLoading(true)
    setMarketDetailError(null)
    setMarketDetail(null)

    mcpGetMarketplaceServerDetail({
      providerId: selectedMarketItem.provider_id,
      serverId: selectedMarketItem.server_id,
    })
      .then((detail) => {
        if (cancelled) return
        setMarketDetail(detail)
        const defaultOption =
          detail.install_options.find(
            (item) => item.id === detail.default_option_id
          ) ??
          detail.install_options[0] ??
          null
        setSelectedInstallOptionId(defaultOption?.id ?? "")
        setInstallParamDraft(defaultParamDraft(defaultOption))
        setMarketSpecText(
          JSON.stringify(defaultOption?.spec ?? detail.spec, null, 2)
        )
        setMarketSpecDirty(false)
      })
      .catch((err) => {
        if (cancelled) return
        const message = toLocalizedErrorMessage(err, mcpT)
        setMarketDetailError(message)
        setMarketDetail(null)
        setMarketSpecText("")
        setMarketSpecDirty(false)
        setSelectedInstallOptionId("")
        setInstallParamDraft({})
      })
      .finally(() => {
        if (!cancelled) setMarketDetailLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [selection, selectedMarketItem, mcpT])

  const executeSearch = useCallback(
    async ({
      providerId,
      query,
    }: {
      providerId: string
      query: string | null
    }) => {
      if (!providerId) return

      const requestId = ++marketSearchRequestRef.current
      setSearching(true)
      setSearchError(null)
      setSearchResults([])
      setSelection((current) => (current?.kind === "market" ? null : current))

      try {
        const results = await mcpSearchMarketplace({
          providerId,
          query: query?.trim() || null,
          limit: 30,
        })
        if (requestId !== marketSearchRequestRef.current) return
        setSearchResults(results)

        if (results[0]) {
          setSelection((current) => {
            if (current?.kind === "market") {
              const hit = results.some((item) => item.server_id === current.id)
              if (hit) return current
            }
            return { kind: "market", id: results[0].server_id }
          })
        }
      } catch (err) {
        if (requestId !== marketSearchRequestRef.current) return
        const message = toLocalizedErrorMessage(err, mcpT)
        setSearchError(message)
      } finally {
        if (requestId === marketSearchRequestRef.current) setSearching(false)
      }
    },
    [mcpT]
  )

  useEffect(() => {
    marketQueryRef.current = marketQuery
  }, [marketQuery])

  useEffect(() => {
    if (leftTab !== "market" || !selectedProvider) return
    executeSearch({
      providerId: selectedProvider,
      query: marketQueryRef.current,
    }).catch((err) => {
      console.error("[Settings] auto search MCP marketplace failed:", err)
    })
    return () => {
      marketSearchRequestRef.current += 1
    }
  }, [executeSearch, leftTab, selectedProvider])

  const uninstallServer = useCallback(
    async (serverId: string) => {
      const action = `uninstall:${serverId}`
      setRunningAction(action)

      try {
        await mcpRemoveServer(serverId)
        const next = await refreshLocalServers()
        toast.success(t("toasts.uninstalled"))

        setSelection((current) => {
          if (current?.kind !== "local" || current.id !== serverId)
            return current
          if (next[0]) return { kind: "local", id: next[0].id }
          return null
        })
      } catch (err) {
        const message = toLocalizedErrorMessage(err, mcpT)
        toast.error(t("toasts.uninstallFailed", { message }))
      } finally {
        setRunningAction(null)
      }
    },
    [refreshLocalServers, t, mcpT]
  )

  const toggleServerEnabled = useCallback(
    async (server: LocalMcpServer, enabled: boolean) => {
      const action = `toggle:${server.id}`
      setRunningAction(action)

      try {
        await mcpSetServerEnabled(server.id, enabled)
        await refreshLocalServers()
      } catch (err) {
        const message = toLocalizedErrorMessage(err, mcpT)
        toast.error(t("toasts.saveFailed", { message }))
      } finally {
        setRunningAction(null)
      }
    },
    [mcpT, refreshLocalServers, t]
  )

  const saveLocalServer = useCallback(async () => {
    if (!selectedLocal) return

    let parsedSpec: Record<string, unknown>
    try {
      parsedSpec = parseJsonObject(
        localSpecText,
        t("jsonNames.localConfig"),
        mcpT
      )
    } catch (err) {
      const message = toLocalizedErrorMessage(err, mcpT)
      toast.error(message)
      return
    }

    const action = `save:${selectedLocal.id}`
    setRunningAction(action)

    try {
      await mcpUpsertLocalServer({
        serverId: selectedLocal.id,
        spec: parsedSpec,
      })
      const next = await refreshLocalServers()
      toast.success(t("toasts.saveSuccess"))

      const updated = next.find((item) => item.id === selectedLocal.id)
      if (updated) {
        setSelection({ kind: "local", id: updated.id })
        setLocalSpecText(JSON.stringify(updated.spec, null, 2))
      }
    } catch (err) {
      const message = toLocalizedErrorMessage(err, mcpT)
      toast.error(t("toasts.saveFailed", { message }))
    } finally {
      setRunningAction(null)
    }
  }, [localSpecText, mcpT, refreshLocalServers, selectedLocal, t])

  const handleCreateDraft = useCallback(() => {
    setLeftTab("custom")
    setSelection({ kind: "draft" })
    setDraftServerId("")
    setDraftSpecText(DEFAULT_DRAFT_SPEC)
  }, [])

  const saveDraft = useCallback(async () => {
    const trimmedId = draftServerId.trim()
    if (!trimmedId) {
      toast.error(t("toasts.serverIdRequired"))
      return
    }

    if (installedServers.some((server) => server.id === trimmedId)) {
      toast.error(t("toasts.serverIdExists", { id: trimmedId }))
      return
    }

    let parsedSpec: Record<string, unknown>
    try {
      parsedSpec = parseJsonObject(
        draftSpecText,
        t("jsonNames.localConfig"),
        mcpT
      )
    } catch (err) {
      const message = toLocalizedErrorMessage(err, mcpT)
      toast.error(message)
      return
    }

    const action = `create:${trimmedId}`
    setRunningAction(action)

    try {
      await mcpUpsertLocalServer({
        serverId: trimmedId,
        spec: parsedSpec,
      })
      const next = await refreshLocalServers()
      toast.success(t("toasts.created"))

      const created = next.find((item) => item.id === trimmedId)
      if (created) {
        setSelection({ kind: "local", id: created.id })
      } else {
        setSelection(null)
      }
    } catch (err) {
      const message = toLocalizedErrorMessage(err, mcpT)
      toast.error(t("toasts.saveFailed", { message }))
    } finally {
      setRunningAction(null)
    }
  }, [
    draftServerId,
    draftSpecText,
    installedServers,
    mcpT,
    refreshLocalServers,
    t,
  ])

  const switchInstallOption = useCallback(
    (optionId: string) => {
      if (!marketDetail) return
      const option =
        marketDetail.install_options.find((item) => item.id === optionId) ??
        marketDetail.install_options[0] ??
        null
      setSelectedInstallOptionId(option?.id ?? "")
      setInstallParamDraft(defaultParamDraft(option))
      setMarketSpecText(
        JSON.stringify(option?.spec ?? marketDetail.spec, null, 2)
      )
      setMarketSpecDirty(false)
    },
    [marketDetail]
  )

  const openInstallDialog = useCallback(() => {
    if (!marketDetail) return
    const option =
      marketDetail.install_options.find(
        (item) => item.id === selectedInstallOptionId
      ) ??
      marketDetail.install_options[0] ??
      null
    setSelectedInstallOptionId(option?.id ?? "")
    setInstallParamDraft(defaultParamDraft(option))
    setInstallDialogOpen(true)
  }, [marketDetail, selectedInstallOptionId])

  const installMarketServer = useCallback(async () => {
    if (!marketDetail) return

    const parsedParams = parseParameterValues(
      selectedInstallOption,
      installParamDraft,
      mcpT
    )
    if (parsedParams.error) {
      toast.error(parsedParams.error)
      return
    }

    let specOverride: Record<string, unknown> | null = null
    const baselineText = JSON.stringify(
      selectedInstallOption?.spec ?? marketDetail.spec,
      null,
      2
    ).trim()
    const currentSpecText = marketSpecText.trim()
    if (marketSpecDirty && currentSpecText !== baselineText) {
      try {
        specOverride = parseJsonObject(
          marketSpecText,
          t("jsonNames.installConfig"),
          mcpT
        )
      } catch (err) {
        const message = toLocalizedErrorMessage(err, mcpT)
        toast.error(message)
        return
      }
    }

    const action = `install:${marketDetail.server_id}`
    setRunningAction(action)

    try {
      await mcpInstallFromMarketplace({
        providerId: marketDetail.provider_id,
        serverId: marketDetail.server_id,
        optionId: selectedInstallOption?.id ?? null,
        protocol: selectedInstallOption?.protocol ?? null,
        parameterValues: parsedParams.values,
        specOverride,
      })
      const nextLocal = await refreshLocalServers()
      toast.success(t("toasts.installed", { name: marketDetail.name }))
      setInstallDialogOpen(false)
      setLeftTab("local")

      const installed = nextLocal.find(
        (item) => item.id === marketDetail.server_id
      )
      if (installed) {
        setSelection({ kind: "local", id: installed.id })
      }
    } catch (err) {
      const message = toLocalizedErrorMessage(err, mcpT)
      toast.error(t("toasts.installFailed", { message }))
    } finally {
      setRunningAction(null)
    }
  }, [
    installParamDraft,
    marketDetail,
    marketSpecDirty,
    marketSpecText,
    mcpT,
    refreshLocalServers,
    selectedInstallOption,
    t,
  ])

  if (loading) {
    return (
      <div className="h-full flex items-center justify-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="h-4 w-4 animate-spin" />
        {t("loading")}
      </div>
    )
  }

  return (
    <>
      <Dialog open={installDialogOpen} onOpenChange={setInstallDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("installDialog.title")}</DialogTitle>
            <DialogDescription>
              {marketDetail
                ? t("installDialog.descriptionWithName", {
                    name: marketDetail.name,
                  })
                : t("installDialog.description")}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 text-sm">
            <div className="space-y-2">
              <div className="text-xs text-muted-foreground">
                {t("installDialog.protocol")}
              </div>
              <Select
                value={selectedInstallOption?.id ?? ""}
                onValueChange={switchInstallOption}
              >
                <SelectTrigger>
                  <SelectValue
                    placeholder={t("installDialog.selectProtocol")}
                  />
                </SelectTrigger>
                <SelectContent>
                  {(marketDetail?.install_options ?? []).map((option) => (
                    <SelectItem key={option.id} value={option.id}>
                      {protocolBadgeLabel(option.protocol, mcpT)} ·{" "}
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {selectedInstallOption?.parameters.length ? (
              <div className="space-y-2">
                <div className="text-xs text-muted-foreground">
                  {t("installDialog.parameters")}
                </div>
                <div className="max-h-56 overflow-auto space-y-2 pr-1">
                  {selectedInstallOption.parameters.map((field) => {
                    const raw = installParamDraft[field.key] ?? ""
                    return (
                      <div key={field.key} className="space-y-1">
                        <div className="text-xs font-medium">
                          {field.label}
                          {field.required ? (
                            <span className="text-red-500 ml-1">*</span>
                          ) : null}
                          {field.location ? (
                            <span className="text-muted-foreground ml-2">
                              {field.location}
                            </span>
                          ) : null}
                        </div>
                        {field.kind === "boolean" ? (
                          <Select
                            value={raw}
                            onValueChange={(value) =>
                              setInstallParamDraft((prev) => ({
                                ...prev,
                                [field.key]: value,
                              }))
                            }
                          >
                            <SelectTrigger>
                              <SelectValue
                                placeholder={t(
                                  "installDialog.booleanPlaceholder"
                                )}
                              />
                            </SelectTrigger>
                            <SelectContent>
                              <SelectItem value="true">true</SelectItem>
                              <SelectItem value="false">false</SelectItem>
                            </SelectContent>
                          </Select>
                        ) : field.enum_values.length > 0 ? (
                          <Select
                            value={raw}
                            onValueChange={(value) =>
                              setInstallParamDraft((prev) => ({
                                ...prev,
                                [field.key]: value,
                              }))
                            }
                          >
                            <SelectTrigger>
                              <SelectValue
                                placeholder={t("installDialog.selectOneValue")}
                              />
                            </SelectTrigger>
                            <SelectContent>
                              {field.enum_values.map((value) => (
                                <SelectItem key={value} value={value}>
                                  {value}
                                </SelectItem>
                              ))}
                            </SelectContent>
                          </Select>
                        ) : field.kind === "json" ? (
                          <Textarea
                            value={raw}
                            onChange={(event) =>
                              setInstallParamDraft((prev) => ({
                                ...prev,
                                [field.key]: event.target.value,
                              }))
                            }
                            className="min-h-20 font-mono text-xs"
                            placeholder={field.placeholder ?? ""}
                          />
                        ) : (
                          <Input
                            type={field.secret ? "password" : "text"}
                            value={raw}
                            onChange={(event) =>
                              setInstallParamDraft((prev) => ({
                                ...prev,
                                [field.key]: event.target.value,
                              }))
                            }
                            placeholder={field.placeholder ?? ""}
                          />
                        )}
                        {field.description ? (
                          <div className="text-[11px] text-muted-foreground leading-5">
                            {field.description}
                          </div>
                        ) : null}
                      </div>
                    )
                  })}
                </div>
              </div>
            ) : null}
          </div>

          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setInstallDialogOpen(false)}
              disabled={Boolean(runningAction?.startsWith("install:"))}
            >
              {t("actions.cancel")}
            </Button>
            <Button
              onClick={() => {
                installMarketServer().catch((err) => {
                  console.error("[Settings] install MCP failed:", err)
                })
              }}
              disabled={Boolean(runningAction?.startsWith("install:"))}
            >
              {runningAction?.startsWith("install:") ? (
                <>
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  {t("actions.installing")}
                </>
              ) : (
                t("actions.confirmInstall")
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <div
        className={cn(
          "grid h-full min-h-0 min-w-0 grid-cols-1 lg:grid-cols-[minmax(18rem,22rem)_minmax(0,1fr)]",
          embedded
            ? "grid-rows-[minmax(22rem,45%)_minmax(22rem,1fr)] gap-0 overflow-auto lg:grid-rows-1 lg:overflow-hidden"
            : "gap-4 px-5 py-5"
        )}
      >
        <section
          className={cn(
            "min-h-0 min-w-0 bg-background p-3",
            embedded
              ? "border-b lg:border-b-0 lg:border-r"
              : "rounded-xl border"
          )}
        >
          <Tabs
            value={leftTab}
            onValueChange={(value) => {
              marketSearchRequestRef.current += 1
              if (value === "custom") {
                handleCreateDraft()
                return
              }
              setLeftTab(value as LeftTab)
              setSelection(
                value === "local" && installedServers[0]
                  ? { kind: "local", id: installedServers[0].id }
                  : null
              )
            }}
            className="h-full min-h-0 gap-0"
          >
            <TabsList variant="line" className="w-full shrink-0 border-b pb-2">
              <TabsTrigger value="local" className="flex-1">
                {t("tabs.local")}
              </TabsTrigger>
              <TabsTrigger value="market" className="flex-1">
                {t("tabs.market")}
              </TabsTrigger>
              <TabsTrigger value="custom" className="flex-1">
                {t("tabs.custom")}
              </TabsTrigger>
            </TabsList>

            <TabsContent
              value="local"
              className="flex min-h-0 flex-1 flex-col pt-3"
            >
              <div className="relative shrink-0 pb-3">
                <Search
                  className="pointer-events-none absolute left-3 top-2.5 size-3.5 text-muted-foreground"
                  aria-hidden="true"
                />
                <Input
                  className="h-9 bg-muted/20 pl-8 shadow-none"
                  value={localFilter}
                  onChange={(event) => setLocalFilter(event.target.value)}
                  placeholder={t("local.filterPlaceholder")}
                  aria-label={t("local.filterPlaceholder")}
                />
              </div>

              {loadingError ? (
                <div className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400">
                  {t("local.loadFailed", { message: loadingError })}
                </div>
              ) : null}

              <div className="flex-1 min-h-0 overflow-auto space-y-1">
                {filteredLocalServers.length === 0 ? (
                  <div className="rounded-md border border-dashed p-3 text-xs text-muted-foreground">
                    {t("local.empty")}
                  </div>
                ) : (
                  filteredLocalServers.map((server) => {
                    const active =
                      selection?.kind === "local" && selection.id === server.id
                    const spec = isObject(server.spec) ? server.spec : {}
                    const sources = pluginSources(server)
                    const needsConfig = (server.missing_config ?? []).length > 0
                    return (
                      <ContextMenu key={server.id}>
                        <ContextMenuTrigger asChild>
                          <div
                            className={cn(
                              "flex w-full items-center gap-3 rounded-md border border-transparent p-3 transition-colors",
                              active
                                ? "border-primary bg-primary/5"
                                : "hover:bg-muted/60"
                            )}
                          >
                            <button
                              type="button"
                              className="flex min-w-0 flex-1 items-start gap-3 rounded-sm text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                              aria-current={active ? "true" : undefined}
                              onClick={() => {
                                setSelection({ kind: "local", id: server.id })
                              }}
                            >
                              <MarketItemIcon name={connectorName(server)} />
                              <div className="min-w-0 flex-1">
                                <div
                                  className="truncate text-sm font-medium"
                                  title={connectorName(server)}
                                >
                                  {connectorName(server)}
                                </div>
                                {connectorName(server) !== server.id ? (
                                  <div className="truncate font-mono text-[10px] text-muted-foreground">
                                    {server.id}
                                  </div>
                                ) : null}
                                <div className="mt-1 line-clamp-2 break-words text-xs leading-5 text-muted-foreground">
                                  {server.description ||
                                    specSummary(spec, mcpT)}
                                </div>
                                <ConnectorListMetadata server={server} />
                              </div>
                            </button>
                            <Switch
                              checked={server.enabled}
                              onCheckedChange={(enabled) => {
                                toggleServerEnabled(server, enabled).catch(
                                  (err) => {
                                    console.error(
                                      "[Settings] toggle MCP failed:",
                                      err
                                    )
                                  }
                                )
                              }}
                              disabled={
                                runningAction !== null ||
                                (!server.enabled && needsConfig)
                              }
                              aria-label={t("local.globalToggle", {
                                name: connectorName(server),
                              })}
                              title={t("local.globalToggle", {
                                name: connectorName(server),
                              })}
                            />
                          </div>
                        </ContextMenuTrigger>
                        <ContextMenuContent>
                          {sources.length === 0 ? (
                            <ContextMenuItem
                              variant="destructive"
                              onClick={() => {
                                uninstallServer(server.id).catch((err) => {
                                  console.error(
                                    "[Settings] uninstall connector failed:",
                                    err
                                  )
                                })
                              }}
                            >
                              {t("actions.uninstall")}
                            </ContextMenuItem>
                          ) : null}
                        </ContextMenuContent>
                      </ContextMenu>
                    )
                  })
                )}
              </div>

              <div className="mt-2 flex shrink-0 items-center justify-end gap-2 border-t pt-2">
                <Button
                  size="icon-sm"
                  variant="ghost"
                  aria-label={t("actions.refresh")}
                  title={t("actions.refresh")}
                  onClick={() => {
                    refreshLocalServers().catch((err) => {
                      console.error("[Settings] refresh local MCP failed:", err)
                    })
                  }}
                >
                  <RefreshCw className="h-3.5 w-3.5" />
                </Button>
              </div>
            </TabsContent>

            <TabsContent
              value="market"
              className="flex min-h-0 flex-1 flex-col pt-3"
            >
              <div className="shrink-0 space-y-2 pb-3">
                <Select
                  value={selectedProvider}
                  onValueChange={(value) => {
                    marketSearchRequestRef.current += 1
                    setSearchResults([])
                    setSelection(null)
                    setSelectedProvider(value)
                  }}
                >
                  <SelectTrigger
                    className="h-9 w-full"
                    aria-label={t("market.selectMarketplace")}
                  >
                    <SelectValue placeholder={t("market.selectMarketplace")} />
                  </SelectTrigger>
                  <SelectContent>
                    {providers.map((provider) => (
                      <SelectItem key={provider.id} value={provider.id}>
                        {provider.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>

                <div className="flex gap-2">
                  <Input
                    className="h-9 min-w-0 bg-muted/20 shadow-none"
                    value={marketQuery}
                    onChange={(event) => setMarketQuery(event.target.value)}
                    placeholder={t("market.searchPlaceholder")}
                    aria-label={t("market.searchPlaceholder")}
                    onKeyDown={(event) => {
                      if (event.key !== "Enter") return
                      executeSearch({
                        providerId: selectedProvider,
                        query: marketQuery,
                      }).catch((err) => {
                        console.error(
                          "[Settings] search MCP marketplace failed:",
                          err
                        )
                      })
                    }}
                  />
                  <Button
                    size="icon"
                    variant="outline"
                    aria-label={t("market.searchPlaceholder")}
                    title={t("market.searchPlaceholder")}
                    onClick={() => {
                      executeSearch({
                        providerId: selectedProvider,
                        query: marketQuery,
                      }).catch((err) => {
                        console.error(
                          "[Settings] search MCP marketplace failed:",
                          err
                        )
                      })
                    }}
                    disabled={searching || !selectedProvider}
                  >
                    {searching ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <Search className="h-3.5 w-3.5" />
                    )}
                  </Button>
                </div>
              </div>

              {searchError ? (
                <div className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400">
                  {t("market.searchFailed", { message: searchError })}
                </div>
              ) : null}

              <div className="min-h-0 flex-1 space-y-1 overflow-auto">
                {searching ? (
                  <div className="h-full min-h-24 rounded-md border border-dashed flex items-center justify-center gap-2 text-xs text-muted-foreground">
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    {t("market.loadingList")}
                  </div>
                ) : searchResults.length === 0 ? (
                  <div className="rounded-md border border-dashed p-3 text-xs text-muted-foreground">
                    {t("market.empty")}
                  </div>
                ) : (
                  searchResults.map((item) => {
                    const active =
                      selection?.kind === "market" &&
                      selection.id === item.server_id
                    return (
                      <ContextMenu
                        key={`${item.provider_id}:${item.server_id}`}
                      >
                        <ContextMenuTrigger asChild>
                          <button
                            type="button"
                            aria-current={active ? "true" : undefined}
                            className={cn(
                              "w-full rounded-md border border-transparent p-3 text-left outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/50",
                              active
                                ? "border-primary bg-primary/5"
                                : "hover:bg-muted/60"
                            )}
                            onClick={() => {
                              setSelection({
                                kind: "market",
                                id: item.server_id,
                              })
                            }}
                          >
                            <div className="flex items-start gap-3">
                              <MarketItemIcon
                                name={item.name}
                                src={item.icon_url}
                              />
                              <div className="min-w-0 flex-1">
                                <div
                                  className="truncate text-sm font-medium"
                                  title={item.name}
                                >
                                  {item.name}
                                </div>
                                <div className="mt-0.5 truncate text-[10px] text-muted-foreground">
                                  {item.server_id}
                                </div>
                              </div>
                            </div>
                            {item.description ? (
                              <p className="mt-2 line-clamp-2 break-words text-xs leading-5 text-muted-foreground">
                                {item.description}
                              </p>
                            ) : null}
                            <div className="mt-2 flex flex-wrap gap-1">
                              {item.protocols.map((protocol) => (
                                <Badge
                                  key={`${item.server_id}-${protocol}`}
                                  variant="secondary"
                                  className="text-[10px]"
                                >
                                  {protocolBadgeLabel(protocol, mcpT)}
                                </Badge>
                              ))}
                              {item.latest_version ? (
                                <Badge
                                  variant="outline"
                                  className="text-[10px]"
                                >
                                  v{item.latest_version}
                                </Badge>
                              ) : null}
                              {item.verified ? (
                                <Badge className="text-[10px]">
                                  {t("badges.verified")}
                                </Badge>
                              ) : null}
                              {typeof item.downloads === "number" ? (
                                <Badge
                                  variant="outline"
                                  className="text-[10px]"
                                >
                                  {t("badges.uses", { count: item.downloads })}
                                </Badge>
                              ) : null}
                            </div>
                          </button>
                        </ContextMenuTrigger>
                        <ContextMenuContent>
                          <ContextMenuItem
                            onClick={() => {
                              setSelection({
                                kind: "market",
                                id: item.server_id,
                              })
                            }}
                          >
                            {t("actions.viewDetails")}
                          </ContextMenuItem>
                        </ContextMenuContent>
                      </ContextMenu>
                    )
                  })
                )}
              </div>
            </TabsContent>

            <TabsContent value="custom" className="min-h-0 flex-1 pt-3">
              <div className="flex h-full flex-col items-center justify-center gap-3 border border-dashed p-5 text-center">
                <Plus className="size-5 text-muted-foreground" />
                <p className="text-xs text-muted-foreground">
                  {t("custom.description")}
                </p>
                <Button size="sm" onClick={handleCreateDraft}>
                  {t("actions.newConnector")}
                </Button>
              </div>
            </TabsContent>
          </Tabs>
        </section>

        <section
          className={cn(
            "min-h-0 min-w-0 overflow-auto bg-background p-4 sm:p-5",
            !embedded && "rounded-xl border"
          )}
        >
          {selection?.kind === "draft" ? (
            <div className="space-y-4">
              <div>
                <h2 className="text-base font-semibold">
                  {t("local.draftTitle")}
                </h2>
                <p className="text-xs text-muted-foreground mt-1">
                  {t("local.draftDescription")}
                </p>
              </div>

              <div className="space-y-2">
                <div className="text-xs text-muted-foreground">
                  {t("local.serverIdLabel")}
                </div>
                <Input
                  value={draftServerId}
                  onChange={(event) => setDraftServerId(event.target.value)}
                  placeholder={t("local.serverIdPlaceholder")}
                />
              </div>

              <div className="space-y-2">
                <div className="text-xs text-muted-foreground">
                  {t("local.configJson")}
                </div>
                <p className="text-xs text-muted-foreground">
                  {t("local.typeHint")}
                </p>
                <Textarea
                  value={draftSpecText}
                  onChange={(event) => setDraftSpecText(event.target.value)}
                  className="min-h-[360px] font-mono text-xs"
                />
              </div>

              {draftEnvOnRemote ? (
                <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-600 dark:text-amber-400">
                  {t("local.envOnRemoteWarning")}
                </div>
              ) : null}

              <div className="flex justify-end gap-2">
                <Button
                  variant="outline"
                  onClick={() => setSelection(null)}
                  disabled={Boolean(runningAction?.startsWith("create:"))}
                >
                  {t("actions.cancel")}
                </Button>
                <Button
                  onClick={() => {
                    saveDraft().catch((err) => {
                      console.error("[Settings] create local MCP failed:", err)
                    })
                  }}
                  disabled={Boolean(runningAction?.startsWith("create:"))}
                >
                  {runningAction?.startsWith("create:") ? (
                    <>
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      {t("actions.creating")}
                    </>
                  ) : (
                    t("actions.create")
                  )}
                </Button>
              </div>
            </div>
          ) : null}

          {selection?.kind === "local" && selectedLocal ? (
            <div className="space-y-4">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <h2 className="text-base font-semibold break-all">
                    {connectorName(selectedLocal)}
                  </h2>
                  <p className="font-mono text-[10px] text-muted-foreground">
                    {selectedLocal.id}
                  </p>
                  <p className="text-xs text-muted-foreground mt-1">
                    {t("local.description")}
                  </p>
                </div>
                {pluginSources(selectedLocal).length === 0 ? (
                  <Button
                    variant="destructive"
                    onClick={() => {
                      uninstallServer(selectedLocal.id).catch((err) => {
                        console.error(
                          "[Settings] uninstall connector failed:",
                          err
                        )
                      })
                    }}
                    disabled={runningAction === `uninstall:${selectedLocal.id}`}
                  >
                    {runningAction === `uninstall:${selectedLocal.id}` ? (
                      <>
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                        {t("actions.uninstalling")}
                      </>
                    ) : (
                      t("actions.uninstall")
                    )}
                  </Button>
                ) : null}
              </div>

              <ConnectorDetailMetadata server={selectedLocal} />

              <div className="space-y-2">
                <div className="text-xs text-muted-foreground">
                  {t("local.configJson")}
                </div>
                <p className="text-xs text-muted-foreground">
                  {t("local.typeHint")}
                </p>
                <Textarea
                  value={localSpecText}
                  onChange={(event) => setLocalSpecText(event.target.value)}
                  className="min-h-[360px] font-mono text-xs"
                />
              </div>

              {localEnvOnRemote ? (
                <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-600 dark:text-amber-400">
                  {t("local.envOnRemoteWarning")}
                </div>
              ) : null}

              <div className="flex justify-end">
                <Button
                  onClick={() => {
                    saveLocalServer().catch((err) => {
                      console.error("[Settings] save local MCP failed:", err)
                    })
                  }}
                  disabled={runningAction === `save:${selectedLocal.id}`}
                >
                  {runningAction === `save:${selectedLocal.id}` ? (
                    <>
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      {t("actions.saving")}
                    </>
                  ) : (
                    t("actions.save")
                  )}
                </Button>
              </div>
            </div>
          ) : null}

          {selection?.kind === "market" ? (
            <div className="space-y-4">
              {marketDetailLoading ? (
                <div className="h-40 flex items-center justify-center gap-2 text-sm text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t("market.loadingDetail")}
                </div>
              ) : marketDetailError ? (
                <div className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400">
                  {t("market.detailLoadFailed", { message: marketDetailError })}
                </div>
              ) : marketDetail ? (
                <>
                  <div className="flex items-start justify-between gap-3">
                    <div className="flex items-start gap-3 min-w-0">
                      <MarketItemIcon
                        name={marketDetail.name}
                        src={marketDetail.icon_url}
                        className="size-12"
                      />
                      <div className="min-w-0">
                        <h2 className="text-base font-semibold break-all">
                          {marketDetail.name}
                        </h2>
                        <p className="text-xs text-muted-foreground break-all mt-1">
                          {marketDetail.server_id}
                        </p>
                      </div>
                    </div>
                    <Button onClick={openInstallDialog}>
                      {t("actions.install")}
                    </Button>
                  </div>

                  <div className="flex flex-wrap gap-1.5">
                    {marketDetail.verified ? (
                      <Badge>{t("badges.verified")}</Badge>
                    ) : null}
                    {marketDetail.remote ? (
                      <Badge variant="secondary">{t("badges.remote")}</Badge>
                    ) : null}
                    {marketDetail.homepage ? (
                      <Badge variant="outline">{t("badges.hasHomepage")}</Badge>
                    ) : null}
                    {marketDetail.protocols.map((protocol) => (
                      <Badge key={`detail-${protocol}`} variant="secondary">
                        {protocolBadgeLabel(protocol, mcpT)}
                      </Badge>
                    ))}
                    {marketDetail.latest_version ? (
                      <Badge variant="outline">
                        v{marketDetail.latest_version}
                      </Badge>
                    ) : null}
                    {typeof marketDetail.downloads === "number" ? (
                      <Badge variant="outline">
                        {t("badges.uses", { count: marketDetail.downloads })}
                      </Badge>
                    ) : null}
                  </div>

                  <p className="text-sm text-muted-foreground leading-6">
                    {marketDetail.description}
                  </p>

                  {marketDetail.homepage ? (
                    <a
                      href={marketDetail.homepage}
                      target="_blank"
                      rel="noreferrer"
                      className="text-xs text-primary underline break-all"
                    >
                      {marketDetail.homepage}
                    </a>
                  ) : null}

                  <div className="grid gap-2 text-xs text-muted-foreground sm:grid-cols-2">
                    {marketDetail.owner ? (
                      <div className="inline-flex items-center gap-1.5">
                        <ShieldCheck className="h-3.5 w-3.5" />
                        {t("market.owner", { owner: marketDetail.owner })}
                      </div>
                    ) : null}
                    {marketDetail.namespace ? (
                      <div className="inline-flex items-center gap-1.5">
                        <TerminalSquare className="h-3.5 w-3.5" />
                        {t("market.namespace", {
                          namespace: marketDetail.namespace,
                        })}
                      </div>
                    ) : null}
                    {marketDetail.is_deployed != null ? (
                      <div className="inline-flex items-center gap-1.5">
                        <Globe className="h-3.5 w-3.5" />
                        {marketDetail.is_deployed
                          ? t("badges.deployed")
                          : t("badges.notDeployed")}
                      </div>
                    ) : null}
                  </div>

                  <div className="space-y-2">
                    <div className="text-xs text-muted-foreground">
                      {t("market.defaultInstallProtocol")}
                    </div>
                    <Select
                      value={selectedInstallOption?.id ?? ""}
                      onValueChange={switchInstallOption}
                    >
                      <SelectTrigger>
                        <SelectValue
                          placeholder={t("installDialog.selectProtocol")}
                        />
                      </SelectTrigger>
                      <SelectContent>
                        {marketDetail.install_options.map((option) => (
                          <SelectItem key={option.id} value={option.id}>
                            {protocolBadgeLabel(option.protocol, mcpT)} ·{" "}
                            {option.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                    <div className="text-[11px] text-muted-foreground">
                      {t("market.currentOptionParameterCount", {
                        count: selectedInstallOption?.parameters.length ?? 0,
                      })}
                    </div>
                  </div>

                  <div className="space-y-2">
                    <div className="text-xs text-muted-foreground">
                      {t("market.installConfigDescription")}
                    </div>
                    <Textarea
                      value={marketSpecText}
                      onChange={(event) => {
                        setMarketSpecText(event.target.value)
                        setMarketSpecDirty(true)
                      }}
                      className="min-h-[360px] font-mono text-xs"
                    />
                  </div>
                </>
              ) : (
                <div className="rounded-md border border-dashed p-3 text-xs text-muted-foreground">
                  {t("market.selectLeftToView")}
                </div>
              )}
            </div>
          ) : null}

          {!selection ? (
            <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
              {t("selectLeftMcp")}
            </div>
          ) : null}
        </section>
      </div>
    </>
  )
}
