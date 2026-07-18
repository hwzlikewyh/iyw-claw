import { describe, expect, it, vi } from "vitest"

import {
  buildAgentOptionsSnapshot,
  createGatewayModelCatalog,
  parseGatewayModels,
  reconcileModelConfigValues,
  type GatewayModelPayloadCache,
} from "@/lib/gateway-model-catalog"

function cacheWith(payload: unknown): GatewayModelPayloadCache {
  return {
    read: vi.fn(() => payload),
    write: vi.fn(),
  }
}

describe("gateway model catalog", () => {
  it("parses every valid model returned by the online endpoint", () => {
    const models = parseGatewayModels({
      data: [
        {
          id: "online-alpha",
          display_name: "Online Alpha",
          description: "Primary online model",
          reasoning: {
            efforts: ["low", "high"],
            default_effort: "high",
          },
        },
        {
          id: "online-beta",
          display_name: "Online Beta",
        },
      ],
    })

    expect(models).toEqual([
      {
        id: "online-alpha",
        name: "Online Alpha",
        description: "Primary online model",
        efforts: ["low", "high"],
        defaultEffort: "high",
      },
      {
        id: "online-beta",
        name: "Online Beta",
        description: null,
        efforts: [],
        defaultEffort: null,
      },
    ])
  })

  it("rejects malformed or empty payloads", () => {
    expect(parseGatewayModels(null)).toEqual([])
    expect(parseGatewayModels({ data: [{ id: "" }] })).toEqual([])
    expect(parseGatewayModels({ data: [] })).toEqual([])
  })

  it("uses a fresh online response without merging stale cached models", async () => {
    const cache = cacheWith({
      data: [{ id: "cached-only", display_name: "Cached Only" }],
    })
    const onlinePayload = {
      data: [
        { id: "online-alpha", display_name: "Online Alpha" },
        { id: "online-beta", display_name: "Online Beta" },
      ],
    }
    const catalog = createGatewayModelCatalog({
      fetchModels: vi.fn().mockResolvedValue(onlinePayload),
      cache,
    })

    await expect(catalog.load()).resolves.toMatchObject([
      { id: "online-alpha" },
      { id: "online-beta" },
    ])
    expect(catalog.getCached().map((model) => model.id)).toEqual([
      "online-alpha",
      "online-beta",
    ])
    expect(cache.write).toHaveBeenCalledWith(onlinePayload)
  })

  it("falls back only to the last successful online payload", async () => {
    const cache = cacheWith({
      data: [{ id: "cached-online", display_name: "Cached Online" }],
    })
    const catalog = createGatewayModelCatalog({
      fetchModels: vi.fn().mockRejectedValue(new Error("offline")),
      cache,
    })

    await expect(catalog.load()).resolves.toMatchObject([
      { id: "cached-online" },
    ])
  })

  it("builds model choices exclusively from the online catalog", () => {
    const models = parseGatewayModels({
      data: [
        {
          id: "online-alpha",
          display_name: "Online Alpha",
          reasoning: {
            efforts: ["low", "high"],
            default_effort: "high",
          },
        },
        { id: "online-beta", display_name: "Online Beta" },
      ],
    })

    const snapshot = buildAgentOptionsSnapshot("codex", models, {
      model: "online-beta",
    })
    const model = snapshot.config_options.find(
      (option) => option.id === "model"
    )

    expect(model?.kind.options.map((option) => option.value)).toEqual([
      "online-alpha",
      "online-beta",
    ])
    expect(model?.kind.current_value).toBe("online-beta")
  })

  it("drops stale reasoning settings when the online model changes", () => {
    const models = parseGatewayModels({
      data: [
        {
          id: "reasoning-model",
          reasoning: { efforts: ["low", "high"], default_effort: "high" },
        },
        { id: "plain-model" },
      ],
    })
    const snapshot = buildAgentOptionsSnapshot("codex", models, {
      model: "plain-model",
      reasoning_effort: "high",
    })

    expect(
      reconcileModelConfigValues(snapshot, {
        model: "plain-model",
        reasoning_effort: "high",
      })
    ).toEqual({ model: "plain-model" })
  })
})
