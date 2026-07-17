import { useEffect } from "react"
import { act, render } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
  AcpConnectionsProvider,
  useAcpActions,
  useConnectionStore,
} from "@/contexts/acp-connections-context"
import type { AttachHandlers } from "@/lib/transport/types"
import type { EventEnvelope } from "@/lib/types"

const h = vi.hoisted(() => {
  const attach = vi.fn(() => ({ detach: vi.fn() }))
  return {
    attach,
    stream: { attach },
    actions: null as unknown as ReturnType<typeof useAcpActions> | null,
    store: null as unknown as ReturnType<typeof useConnectionStore> | null,
    acpGetAgentStatus: vi.fn(),
    acpFindConnectionForConversation: vi.fn(),
    acpConnect: vi.fn(),
    acpDisconnect: vi.fn(),
    acpGetSessionSnapshot: vi.fn(),
    denormalizeSnapshot: vi.fn(),
    getFolderConversation: vi.fn(),
    sendSystemNotification: vi.fn(async () => undefined),
  }
})

vi.mock("next-intl", () => ({
  useTranslations: () => (key: string) => key,
}))

vi.mock("@/lib/platform", () => ({
  subscribe: vi.fn(async () => () => {}),
  getEventStream: () => h.stream,
}))

vi.mock("@/lib/delegation-seed", () => ({
  buildDelegationSeedEnvelopes: vi.fn(() => []),
}))

vi.mock("@/contexts/alert-context", () => ({
  useAlertContext: () => ({ pushAlert: vi.fn() }),
}))

vi.mock("@/contexts/active-folder-context", () => ({
  useActiveFolder: () => ({ activeFolder: { path: "/tmp/x", name: "x" } }),
}))

vi.mock("@/lib/notification", () => ({
  sendSystemNotification: h.sendSystemNotification,
}))

vi.mock("@/lib/selector-prefs-storage", () => ({
  getSavedPrefsForConnect: () => ({ modeId: undefined, configValues: {} }),
  saveModePreference: vi.fn(),
  saveConfigPreference: vi.fn(),
}))

vi.mock("@/lib/snapshot-denormalize", () => ({
  denormalizeSnapshot: h.denormalizeSnapshot,
}))

vi.mock("@/lib/api", () => ({
  acpGetAgentStatus: h.acpGetAgentStatus,
  acpFindConnectionForConversation: h.acpFindConnectionForConversation,
  acpConnect: h.acpConnect,
  acpDisconnect: h.acpDisconnect,
  acpGetSessionSnapshot: h.acpGetSessionSnapshot,
  acpPrompt: vi.fn(),
  acpSetMode: vi.fn(),
  acpSetConfigOption: vi.fn(),
  acpCancel: vi.fn(),
  acpRespondPermission: vi.fn(),
  acpAnswerQuestion: vi.fn(),
  acpTouchConnection: vi.fn(),
  getFolderConversation: h.getFolderConversation,
}))

function Probe() {
  const actions = useAcpActions()
  const store = useConnectionStore()
  useEffect(() => {
    h.actions = actions
    h.store = store
  }, [actions, store])
  return null
}

async function mountOwnerConnection(): Promise<AttachHandlers> {
  render(
    <AcpConnectionsProvider>
      <Probe />
    </AcpConnectionsProvider>
  )
  await act(async () => {})
  await act(async () => {
    await h.actions!.connect(
      "conv-1-claude_code-42",
      "claude_code",
      "/tmp/x",
      "sess-1",
      42
    )
  })
  const calls = h.attach.mock.calls as unknown as Array<
    [unknown, unknown, AttachHandlers]
  >
  const handlers = calls[calls.length - 1]?.[2]
  if (!handlers) throw new Error("expected attach handlers")
  return handlers
}

function emit(handlers: AttachHandlers, envelope: EventEnvelope) {
  act(() => handlers.onEvent(envelope))
}

const TAB = "conv-1-claude_code-42"

beforeEach(async () => {
  const { resetConversationRuntimeStore } =
    await import("@/stores/conversation-runtime-store")
  resetConversationRuntimeStore()
  h.attach.mockClear()
  h.actions = null
  h.store = null
  h.acpGetAgentStatus.mockReset().mockResolvedValue({
    agent_type: "claude_code",
    enabled: true,
    available: true,
    installed_version: "1.0.0",
  })
  h.acpFindConnectionForConversation.mockReset().mockResolvedValue(null)
  h.acpConnect.mockReset().mockResolvedValue("spawned-conn")
  h.acpDisconnect.mockReset().mockResolvedValue(undefined)
  h.acpGetSessionSnapshot.mockReset().mockResolvedValue(null)
  h.denormalizeSnapshot.mockReset()
  h.getFolderConversation.mockReset()
  h.sendSystemNotification.mockClear()
})

describe("ACP background activity", () => {
  it("drops out-of-turn deltas but keeps in-turn streaming", async () => {
    const handlers = await mountOwnerConnection()
    emit(handlers, {
      seq: 1,
      connection_id: "spawned-conn",
      type: "status_changed",
      status: "connected",
    })
    emit(handlers, {
      seq: 2,
      connection_id: "spawned-conn",
      type: "content_delta",
      text: "out-of-turn garbage",
    })
    emit(handlers, {
      seq: 3,
      connection_id: "spawned-conn",
      type: "status_changed",
      status: "prompting",
    })
    expect(h.store!.getConnection(TAB)?.liveMessage?.content ?? []).toEqual([])

    emit(handlers, {
      seq: 4,
      connection_id: "spawned-conn",
      type: "content_delta",
      text: "real reply",
    })
    emit(handlers, {
      seq: 5,
      connection_id: "spawned-conn",
      type: "usage_update",
      used: 1,
      size: 100,
    })
    expect(h.store!.getConnection(TAB)?.liveMessage?.content).toEqual([
      { type: "text", text: "real reply" },
    ])
  })

  it("updates outstanding and overlay state without a detail refetch", async () => {
    const { useConversationRuntimeStore } =
      await import("@/stores/conversation-runtime-store")
    useConversationRuntimeStore.getState().actions.setExternalId(-9, "sess-1")
    useConversationRuntimeStore.getState().actions.setDbConversationId(-9, 42)
    const handlers = await mountOwnerConnection()

    emit(handlers, {
      seq: 1,
      connection_id: "spawned-conn",
      type: "background_activity",
      session_id: "sess-1",
      turns: [
        {
          id: "bg-100-0",
          role: "assistant",
          blocks: [{ type: "text", text: "build finished" }],
          timestamp: "2026-07-07T03:47:08.000Z",
        },
      ],
      outstanding: 2,
      settled: [
        {
          task_id: "agent1",
          status: "completed",
          summary: "Build finished",
          tool_use_id: "toolu_01",
          result: "Build OK",
        },
      ],
      watermark: 4096,
    })

    const session = useConversationRuntimeStore
      .getState()
      .byConversationId.get(-9)
    expect(h.store!.getConnection(TAB)?.backgroundOutstanding).toBe(2)
    expect(h.store!.getConnection(TAB)?.backgroundSettleSyncingSince).toEqual(
      expect.any(Number)
    )
    expect(session?.backgroundTurns[0]).toMatchObject({
      watermark: 4096,
      turn: { id: "bg-100-0" },
    })
    expect(session?.pendingBackgroundSettlements).toEqual([
      {
        toolUseId: "toolu_01",
        taskId: "agent1",
        status: "completed",
        summary: "Build finished",
        result: "Build OK",
      },
    ])
    expect(h.getFolderConversation).not.toHaveBeenCalled()
    expect(h.sendSystemNotification).toHaveBeenCalledTimes(1)
  })

  it("does not arm syncing for a wire-visible held-turn settlement", async () => {
    const handlers = await mountOwnerConnection()
    emit(handlers, {
      seq: 1,
      connection_id: "spawned-conn",
      type: "background_activity",
      session_id: "sess-1",
      outstanding: 0,
      settled: [
        {
          task_id: "agent1",
          status: "completed",
          tool_use_id: "toolu_01",
          result: "done",
          wire_visible: true,
        },
      ],
      watermark: 100,
    })

    expect(h.store!.getConnection(TAB)?.backgroundOutstanding).toBe(0)
    expect(h.store!.getConnection(TAB)?.backgroundSettleSyncingSince).toBeNull()
  })
})
