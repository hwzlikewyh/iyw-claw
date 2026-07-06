import { describe, expect, it } from "vitest"
import { defaultRehypePlugins } from "streamdown"

import { rehypePluginsAllowingIywClaw } from "./rehype-allow-iyw-claw"

/** Pull the href protocol allow-list out of a `[rehypeSanitize, schema]` tuple. */
function hrefProtocols(plugin: unknown): string[] | undefined {
  if (!Array.isArray(plugin)) return undefined
  const schema = plugin[1] as { protocols?: { href?: string[] } } | undefined
  return schema?.protocols?.href
}

describe("rehypePluginsAllowingIywClaw", () => {
  it("adds `iyw-claw` to the sanitize schema's href protocol allow-list", () => {
    // Guards against an upstream rename of the `sanitize` key — the whole fix
    // hinges on this entry existing.
    const sanitizeIndex = Object.keys(defaultRehypePlugins).indexOf("sanitize")
    expect(sanitizeIndex).toBeGreaterThanOrEqual(0)

    const href = hrefProtocols(
      rehypePluginsAllowingIywClaw(defaultRehypePlugins)[sanitizeIndex]
    )
    expect(href).toContain("iyw-claw")
    // Exactly once — no duplicate even if re-derived.
    expect(href?.filter((p) => p === "iyw-claw")).toHaveLength(1)
    // Pre-existing protocols are preserved (https is always present).
    expect(href).toContain("https")
  })

  it("preserves plugin count and order, passing raw/harden through by reference", () => {
    const keys = Object.keys(defaultRehypePlugins)
    const result = rehypePluginsAllowingIywClaw(defaultRehypePlugins)
    expect(result).toHaveLength(keys.length)
    keys.forEach((key, i) => {
      if (key !== "sanitize") {
        expect(result[i]).toBe(defaultRehypePlugins[key])
      }
    })
  })

  it("clones rather than mutating the shipped sanitize schema", () => {
    // The shipped default must not already contain iyw-claw, else the fix is moot.
    expect(hrefProtocols(defaultRehypePlugins.sanitize)).not.toContain(
      "iyw-claw"
    )
    rehypePluginsAllowingIywClaw(defaultRehypePlugins)
    // Still absent on the original after deriving — we built a new schema.
    expect(hrefProtocols(defaultRehypePlugins.sanitize)).not.toContain(
      "iyw-claw"
    )
  })
})
