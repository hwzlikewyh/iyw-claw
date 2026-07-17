import { readFileSync } from "node:fs"
import { join } from "node:path"

import { describe, expect, it } from "vitest"

import { RESEARCH_ACTIONS } from "@/lib/research-actions"

function featuredScienceIds(): string[] {
  const source = readFileSync(
    join(process.cwd(), "src-tauri/science/science.toml"),
    "utf8"
  )
  return source
    .split(/^\[\[skill\]\]$/m)
    .slice(1)
    .filter((block) => /^featured\s*=\s*true\s*$/m.test(block))
    .map((block) => /^id\s*=\s*"([^"]+)"/m.exec(block)?.[1])
    .filter((id): id is string => Boolean(id))
    .sort()
}

describe("research actions", () => {
  it("covers every featured science skill exactly once", () => {
    const ids = RESEARCH_ACTIONS.map((action) => action.skillId)
    expect([...ids].sort()).toEqual(featuredScienceIds())
    expect(new Set(ids).size).toBe(ids.length)
  })
})
