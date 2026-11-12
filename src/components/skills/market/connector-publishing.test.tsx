import { act, renderHook, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  connectorUserRequest,
  importConnector,
  newConnectorOption,
  newOwnedConnector,
} from "@/lib/connector-publishing"
import { useConnectorPublishing } from "./use-connector-publishing"
import { useOwnedConnectors } from "./use-owned-connectors"

vi.mock("@/lib/connector-publishing", async (original) => ({
  ...(await original<object>()),
  connectorUserRequest: vi.fn(),
}))
vi.mock("@/lib/connector-publishing-upload", () => ({
  uploadConnectorArtifact: vi.fn(),
}))
const request = vi.mocked(connectorUserRequest)
beforeEach(() => {
  vi.clearAllMocks()
  vi.spyOn(window, "confirm").mockReturnValue(true)
})
afterEach(() => {
  vi.restoreAllMocks()
})

describe("connector declaration import", () => {
  it.each([
    { examples: null },
    { name: {} },
    { install_options: [null] },
    { install_options: [{ ...newConnectorOption(), agents: null }] },
    { install_options: [{ ...newConnectorOption(), spec: [] }] },
    { install_options: [{ ...newConnectorOption(), protocol: "unsupported" }] },
  ])("rejects declarations that cannot be edited safely: %j", (value) => {
    expect(() =>
      importConnector(JSON.stringify({ ...newOwnedConnector(), ...value }))
    ).toThrow()
  })
  it("imports another creator's declaration as a new unpublished draft", () => {
    const draft = importConnector(
      JSON.stringify({
        ...newOwnedConnector(),
        server_id: `user.${"a".repeat(16)}.example`,
        revision: 8,
        listed: true,
        published_version: "1.0.0",
      })
    )
    expect(draft).toMatchObject({
      server_id: "example",
      revision: 0,
      listed: false,
      published_version: "",
    })
  })
})

describe("connector user publication", () => {
  it("keeps form and dirty state when save fails, then reads back the exact saved connector", async () => {
    const changed = vi.fn()
    const { result } = renderHook(() => useConnectorPublishing(changed))
    act(() => result.current.create())
    const draft = {
      ...newOwnedConnector(),
      server_id: "mine",
      name: "Unsaved title",
      description: "description",
      owner: "Publisher",
    }
    act(() => result.current.change(draft))
    request.mockRejectedValueOnce(Error("Save failed"))
    act(() => result.current.save())
    await waitFor(() => expect(result.current.busy).toBe(false))
    expect(result.current.error).toContain("Save failed")
    expect(result.current.draft?.name).toBe("Unsaved title")
    expect(result.current.dirty).toBe(true)
    request.mockResolvedValueOnce({
      ...draft,
      server_id: "user.owner.mine",
      revision: 1,
    })
    act(() => result.current.save())
    await waitFor(() => expect(result.current.dirty).toBe(false))
    expect(result.current.draft?.server_id).toBe("user.owner.mine")
    expect(changed).toHaveBeenCalledTimes(1)
  })
  it("blocks double submit and ignores completion after the account view unmounts", async () => {
    let finish!: (value: unknown) => void
    request.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve
        })
    )
    const changed = vi.fn()
    const { result, unmount } = renderHook(() =>
      useConnectorPublishing(changed)
    )
    act(() => result.current.create())
    act(() => {
      result.current.save()
      result.current.save()
    })
    expect(request).toHaveBeenCalledTimes(1)
    unmount()
    await act(async () => {
      finish({ ...newOwnedConnector(), revision: 1 })
    })
    expect(changed).not.toHaveBeenCalled()
  })
  it("requires saved revision and explicit confirmation before publishing", async () => {
    const { result } = renderHook(() => useConnectorPublishing(vi.fn()))
    act(() => result.current.create())
    act(() => result.current.publish(true))
    await waitFor(() => expect(result.current.busy).toBe(false))
    expect(request).not.toHaveBeenCalled()
    request.mockResolvedValueOnce({ ...newOwnedConnector(), revision: 1 })
    act(() => result.current.save())
    await waitFor(() => expect(result.current.busy).toBe(false))
    vi.mocked(window.confirm).mockReturnValue(false)
    act(() => result.current.publish(true))
    await waitFor(() => expect(result.current.busy).toBe(false))
    expect(request).toHaveBeenCalledTimes(1)
  })
})

describe("my connector uploads pagination", () => {
  it("assembles measured capacities larger than a control API page, retaining manual mode", async () => {
    request.mockImplementation(async (_action, raw) => {
      const input = raw as { page: number; pageSize: number }
      const items = Array.from({ length: 140 }, (_, index) => ({
        ...newOwnedConnector(),
        server_id: String(index),
      }))
      return {
        items: items.slice(
          (input.page - 1) * input.pageSize,
          input.page * input.pageSize
        ),
        total: items.length,
        page: input.page,
        pageSize: input.pageSize,
      } as never
    })
    const { result } = renderHook(useOwnedConnectors)
    act(() => result.current.pagination.onAutoPageSizeChange(54))
    await waitFor(() => expect(result.current.items).toHaveLength(54))
    expect(result.current.pagination.pageSize).toBe(54)
    act(() => result.current.pagination.onPageChange(2))
    await waitFor(() => expect(result.current.items[0]?.server_id).toBe("54"))
    expect(
      result.current.items[result.current.items.length - 1]?.server_id
    ).toBe("107")
    act(() => result.current.pagination.onPageSizeChange(12))
    await waitFor(() => expect(result.current.items).toHaveLength(12))
    expect(result.current.pagination.automaticPageSize).toBe(false)
    act(() => result.current.setSearch("query"))
    expect(result.current.pagination.pageSize).toBe(12)
    expect(result.current.pagination.page).toBe(1)
    await waitFor(() => expect(result.current.loading).toBe(false))
  })
})
