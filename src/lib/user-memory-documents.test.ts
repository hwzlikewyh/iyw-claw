import { describe, expect, it } from "vitest"
import {
  displayUserMemoryPath,
  getUserMemoryDocument,
  userMemoryLineCount,
  userMemoryRelativePath,
  USER_MEMORY_DOCUMENTS,
} from "./user-memory-documents"

describe("user memory documents", () => {
  it("defines the three editable memory documents", () => {
    expect(USER_MEMORY_DOCUMENTS.map((document) => document.id)).toEqual([
      "memory",
      "profile",
      "soul",
    ])
    expect(
      USER_MEMORY_DOCUMENTS.map((document) => userMemoryRelativePath(document))
    ).toEqual([
      ".iyw-claw/user-memory.md",
      ".iyw-claw/user-profile.md",
      ".iyw-claw/user-soul.md",
    ])
  })

  it("formats platform-specific display paths", () => {
    const document = getUserMemoryDocument("profile")
    const relativePath = userMemoryRelativePath(document)

    expect(displayUserMemoryPath("C:\\Users\\Alice\\", relativePath)).toBe(
      "C:\\Users\\Alice\\.iyw-claw\\user-profile.md"
    )
    expect(displayUserMemoryPath("/home/alice", relativePath)).toBe(
      "/home/alice/.iyw-claw/user-profile.md"
    )
  })

  it("counts lines for common line endings", () => {
    expect(userMemoryLineCount("")).toBe(0)
    expect(userMemoryLineCount("a\nb\r\nc")).toBe(3)
  })
})
