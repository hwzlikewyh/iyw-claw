import { createHash } from "node:crypto"
import { afterEach, describe, expect, it, vi } from "vitest"
import { connectorFileDigest } from "./connector-upload-digest"
import { connectorUserRequest } from "./connector-publishing"
import {
  uploadConnectorArtifact,
  type PendingConnectorUpload,
} from "./connector-publishing-upload"
vi.mock("./connector-publishing", () => ({ connectorUserRequest: vi.fn() }))
afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe("bounded connector upload", () => {
  it("matches independent SHA-256 across block and chunk boundaries", async () => {
    for (const size of [0, 1, 55, 56, 63, 64, 65, 4 * 1024 * 1024 + 11]) {
      const bytes = Buffer.alloc(size, 42)
      const file = {
        size,
        slice: (start: number, end: number) => ({
          arrayBuffer: async () => bytes.subarray(start, end),
        }),
      } as unknown as Blob
      const actual = await connectorFileDigest(file, {
        signal: new AbortController().signal,
        progress() {},
      })
      expect(actual).toBe(createHash("sha256").update(bytes).digest("hex"))
    }
  })
  it("retries completion using the same uploaded task without another PUT", async () => {
    const request = vi.mocked(connectorUserRequest)
    request.mockReset()
    request.mockImplementation(async (action) => {
      if (action === "uploads/init")
        return {
          uploadToken: "scoped-task",
          url: "https://managed.example/object",
          method: "PUT",
          expiresAt: new Date(Date.now() + 60_000).toISOString(),
          headers: {
            token: "must-not-send",
            "admin-token": "must-not-send",
            "Content-Length": "4",
          },
        } as never
      throw Error("Registration unavailable")
    })
    const send = vi.fn<
      (...args: [string, RequestInit]) => Promise<{ ok: boolean }>
    >(async () => ({
      ok: true,
    }))
    vi.stubGlobal("fetch", send)
    let pending: PendingConnectorUpload | undefined
    const bytes = Buffer.from("file")
    const file = {
      name: "server.js",
      size: 4,
      slice: (start: number, end: number) => ({
        arrayBuffer: async () => bytes.subarray(start, end),
      }),
    } as unknown as File
    const options = {
      signal: new AbortController().signal,
      status() {},
      retain: (value: PendingConnectorUpload) => {
        pending = value
      },
    }
    await expect(uploadConnectorArtifact(file, options)).rejects.toThrow(
      "Registration unavailable"
    )
    expect(pending?.transferred).toBe(true)
    request.mockResolvedValueOnce({ id: "verified" })
    await expect(
      uploadConnectorArtifact(file, { ...options, pending })
    ).resolves.toEqual({ id: "verified" })
    expect(send).toHaveBeenCalledTimes(1)
    expect(send.mock.calls[0]?.[1]).toMatchObject({
      credentials: "omit",
      redirect: "error",
      headers: {},
    })
    expect(request.mock.calls.map((call) => call[0])).toEqual([
      "uploads/init",
      "uploads/complete",
      "uploads/complete",
    ])
  })
})
