import { act, renderHook, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import {
  mcpListMarketplaces,
  mcpScanLocal,
  mcpSearchMarketplace,
} from "@/lib/api"
import { useConnectorMarket } from "./use-connector-market"
import { useCapabilityCatalog } from "./use-capability-catalog"

vi.mock("@/lib/skill-market-source", () => ({
  getSkillMarketSource: () => ({ list: async () => ({ items: [] }) }),
}))

vi.mock("@/lib/api", () => ({
  mcpListMarketplaces: vi.fn(),
  mcpScanLocal: vi.fn(),
  mcpSearchMarketplace: vi.fn(),
}))
const item = (index: number) => ({
  server_id: String(index),
  provider_id: "iyw_gateway",
})
beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(mcpListMarketplaces).mockResolvedValue([
    { id: "iyw_gateway", name: "爱原物", description: "" },
  ])
  vi.mocked(mcpScanLocal).mockResolvedValue([])
  vi.mocked(mcpSearchMarketplace).mockImplementation(
    async ({ offset = 0, limit = 7 }) =>
      Array.from({ length: limit ?? 7 }, (_, n) =>
        item((offset ?? 0) + n)
      ) as never
  )
})

describe("connector page capacity", () => {
  it("shows structured transport errors in connector and aggregate catalogs", async () => {
    vi.mocked(mcpSearchMarketplace).mockRejectedValue({
      code: "network_error",
      message: "连接器目录暂时无法读取",
      detail: "502 Bad Gateway",
    })
    const connector = renderHook(() => useConnectorMarket("unavailable"))
    const catalog = renderHook(useCapabilityCatalog)
    await waitFor(() => {
      expect(connector.result.current.error).toBe("502 Bad Gateway")
      expect(catalog.result.current.error).toBe("502 Bad Gateway")
    })
    expect(connector.result.current.items).toEqual([])
    expect(catalog.result.current.items).toEqual([])
    connector.unmount()
    catalog.unmount()
  })
  it("requests the measured capacity plus one without inventing totals", async () => {
    const { result, unmount } = renderHook(() =>
      useConnectorMarket("unavailable")
    )
    await waitFor(() => expect(result.current.loading).toBe(false))
    act(() => result.current.setAutoPageSize(9))
    await waitFor(() => expect(result.current.items).toHaveLength(9))
    expect(mcpSearchMarketplace).toHaveBeenLastCalledWith({
      providerId: "iyw_gateway",
      query: null,
      offset: 0,
      limit: 10,
    })
    act(() => result.current.setPage(1))
    await waitFor(() => expect(result.current.loading).toBe(false))
    expect(mcpSearchMarketplace).toHaveBeenLastCalledWith({
      providerId: "iyw_gateway",
      query: null,
      offset: 9,
      limit: 10,
    })
    expect(result.current.hasMore).toBe(true)
    unmount()
  })

  it("retains manual capacity and offers automatic mode again", async () => {
    const { result, unmount } = renderHook(() =>
      useConnectorMarket("unavailable")
    )
    act(() => result.current.setPageSize(12))
    await waitFor(() => expect(result.current.items).toHaveLength(12))
    expect(result.current.automaticPageSize).toBe(false)
    act(() => result.current.setQuery("PDF"))
    expect(result.current.pageSize).toBe(12)
    expect(result.current.page).toBe(0)
    act(() => result.current.setPageSize(0))
    expect(result.current.automaticPageSize).toBe(true)
    unmount()
  })

  it("uses API-sized chunks when a tall screen fits more than fifty", async () => {
    const { result, unmount } = renderHook(() =>
      useConnectorMarket("unavailable")
    )
    act(() => result.current.setAutoPageSize(54))
    await waitFor(() => expect(result.current.items).toHaveLength(54))
    expect(mcpSearchMarketplace).toHaveBeenCalledWith({
      providerId: "iyw_gateway",
      query: null,
      offset: 0,
      limit: 50,
    })
    expect(mcpSearchMarketplace).toHaveBeenLastCalledWith({
      providerId: "iyw_gateway",
      query: null,
      offset: 50,
      limit: 5,
    })
    unmount()
  })

  it("ignores a late response for a superseded capacity", async () => {
    let finish!: (items: never[]) => void
    vi.mocked(mcpSearchMarketplace).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve
        })
    )
    const { result, unmount } = renderHook(() =>
      useConnectorMarket("unavailable")
    )
    await waitFor(() => expect(mcpSearchMarketplace).toHaveBeenCalledTimes(1))
    act(() => result.current.setAutoPageSize(4))
    await waitFor(() => expect(result.current.items).toHaveLength(4))
    await act(async () => {
      finish([])
    })
    expect(result.current.items).toHaveLength(4)
    unmount()
  })
})
