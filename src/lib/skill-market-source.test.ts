import { beforeEach, describe, expect, it, vi } from "vitest"

const call = vi.fn()

vi.mock("@/lib/transport", () => ({
  getTransport: () => ({ call }),
}))

import { getSkillMarketSource } from "@/lib/skill-market-source"

const detail = {
  id: "1",
  slug: "demo",
  displayName: "Demo",
  summary: "summary",
  category: "office-efficiency",
  iconUrl: null,
  tags: [],
  visibility: "private",
  audience: "owner_private",
  publisherType: "official",
  ownedByMe: false,
  canManage: false,
  currentVersion: {
    id: "2",
    version: "1.0.0",
    changelog: null,
    status: "ready",
    fileCount: 0,
    packageSize: 0,
    packageType: "skill",
    dependencies: [],
    createdAt: "2026-01-01T00:00:00Z",
  },
  files: [],
  createdAt: "2026-01-01T00:00:00Z",
  updatedAt: "2026-01-01T00:00:00Z",
}

describe("skill market source metadata updates", () => {
  beforeEach(() => {
    call.mockReset()
    call.mockResolvedValue(detail)
  })

  it("sends visibility and audience together so the pair cannot drift", async () => {
    const source = getSkillMarketSource()
    await source.updateMetadata({
      id: "1",
      displayName: "Demo",
      summary: "summary",
      category: "office-efficiency",
      iconUrl: null,
      tags: [],
      audience: "owner_private",
    })
    expect(call).toHaveBeenCalledWith("skill_market_update_metadata", {
      request: expect.objectContaining({
        visibility: "private",
        audience: "owner_private",
      }),
    })
  })

  it("keeps a global market skill public", async () => {
    const source = getSkillMarketSource()
    await source.updateMetadata({
      id: "1",
      displayName: "Demo",
      summary: "summary",
      category: "office-efficiency",
      iconUrl: null,
      tags: [],
      audience: "global_market",
    })
    expect(call).toHaveBeenCalledWith("skill_market_update_metadata", {
      request: expect.objectContaining({
        visibility: "public",
        audience: "global_market",
      }),
    })
  })
})
