import { describe, expect, it } from "vitest"

import {
  formatAgentRuntimeError,
  type AgentRuntimeErrorMessages,
} from "./agent-runtime-error"

const messages: AgentRuntimeErrorMessages = {
  insufficientBalance: "balance",
  authenticationFailed: "authentication",
  permissionDenied: "permission",
  rateLimited: "rate-limit",
  quotaExceeded: "quota",
  modelUnavailable: "model",
  requestTimeout: "timeout",
  networkError: "network",
  serviceUnavailable: "service",
  requestFailed: "request",
}

describe("formatAgentRuntimeError", () => {
  it("replaces the reported 402 response without exposing transport details", () => {
    const raw =
      "unexpected status 402 Payment Required: Insufficient Balance, " +
      "url: https://gateway.iyw.cn/iyw-fusion-api/v1/responses, " +
      "request id: b76efb0b-b496-43cc-a280-11c29bcf3558"

    const result = formatAgentRuntimeError(raw, messages)

    expect(result).toBe("balance")
    expect(result).not.toContain("gateway.iyw.cn")
    expect(result).not.toContain("b76efb0b")
  })

  it.each([
    ["401 Unauthorized: invalid API key", "authentication"],
    ["403 Forbidden: permission denied", "permission"],
    ["429 Too Many Requests: rate limit exceeded", "rate-limit"],
    ["insufficient_quota: usage limit reached", "quota"],
    ["request timed out after 60 seconds", "timeout"],
    ["network error: connection reset by peer", "network"],
    ["unexpected status 503 Service Unavailable", "service"],
  ])("maps %s to a short user-facing category", (raw, expected) => {
    expect(formatAgentRuntimeError(raw, messages)).toBe(expected)
  })

  it.each([
    ["HTTP 429 Too Many Requests", "rate-limit"],
    ["Error code: 401 - invalid credentials", "authentication"],
    ["APIStatusError: Error code: 500", "service"],
  ])("recognizes common SDK error prefix in %s", (raw, expected) => {
    expect(formatAgentRuntimeError(raw, messages)).toBe(expected)
  })

  it.each([
    ["Insufficient Balance", "balance"],
    [
      "Your account 2102292408 has not activated the model deepseek-v4-pro",
      "model",
    ],
    ['{"error":{"message":"Insufficient Balance"}}', "balance"],
  ])("recognizes an unwrapped provider error in %s", (raw, expected) => {
    expect(formatAgentRuntimeError(raw, messages)).toBe(expected)
  })

  it("uses a generic prompt for an unknown transport error", () => {
    const raw =
      "upstream request failed, url: https://internal.example/v1, " +
      "request id: secret-id"

    expect(formatAgentRuntimeError(raw, messages)).toBe("request")
  })

  it("hides opaque diagnostic fields even without a known error phrase", () => {
    const raw =
      "upstream unavailable, url: https://internal.example/v1, " +
      "request id: ab6fc2b2-6c01-4b44-af2e-6963b80fffbd"

    expect(formatAgentRuntimeError(raw, messages)).toBe("request")
  })

  it("maps an unactivated model response without exposing account details", () => {
    const raw =
      "unexpected status 404 Not Found: Your account 2102292408 has not " +
      "activated the model deepseek-v4-pro-260425. Please activate the model " +
      "service in the Ark Console. Request id: " +
      "02178434654823546d1ea52072bfcd170e4d89d85374428f3fba0, " +
      "url: https://gateway.iyw.cn/iyw-fusion-api/v1/responses, " +
      "request id: ab6fc2b2-6c01-4b44-af2e-6963b80fffbd"

    const result = formatAgentRuntimeError(raw, messages)

    expect(result).toBe("model")
    expect(result).not.toContain("2102292408")
    expect(result).not.toContain("deepseek-v4-pro-260425")
    expect(result).not.toContain("gateway.iyw.cn")
  })

  it("does not rewrite normal assistant content containing a URL or status code", () => {
    const normal =
      "The API documentation is at https://example.com and explains HTTP 429."

    expect(formatAgentRuntimeError(normal, messages)).toBeNull()
  })
})
