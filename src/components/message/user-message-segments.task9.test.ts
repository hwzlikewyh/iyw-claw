import { describe, expect, it } from "vitest"

import { parseUserMessageSegments } from "@/components/message/user-message-segments"

describe("parseUserMessageSegments", () => {
  it("badges local references and bare invocations without changing prose", () => {
    const segments = parseUserMessageSegments(
      "**literal** /build $deploy [main.ts](file:///tmp/main.ts)"
    )

    expect(segments).toEqual([
      { kind: "text", text: "**literal** " },
      expect.objectContaining({
        kind: "reference",
        attrs: expect.objectContaining({ id: "build", label: "build" }),
      }),
      { kind: "text", text: " " },
      expect.objectContaining({
        kind: "reference",
        attrs: expect.objectContaining({ id: "deploy", label: "deploy" }),
      }),
      { kind: "text", text: " " },
      expect.objectContaining({
        kind: "reference",
        attrs: expect.objectContaining({
          refType: "file",
          label: "main.ts",
        }),
      }),
    ])
  })

  it("leaves ordinary links and path-like slash text literal", () => {
    const segments = parseUserMessageSegments(
      "[site](https://example.com) /usr/bin $5"
    )
    expect(segments.every((segment) => segment.kind === "text")).toBe(true)
    expect(
      segments
        .map((segment) => (segment.kind === "text" ? segment.text : ""))
        .join("")
    ).toBe("[site](https://example.com) /usr/bin $5")
  })
})
