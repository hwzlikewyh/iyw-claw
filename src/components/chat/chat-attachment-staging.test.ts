import { describe, expect, it, vi } from "vitest"

import { preparePickedAttachmentPaths } from "./chat-attachment-staging"

describe("preparePickedAttachmentPaths", () => {
  it("stages local files into the Chat working directory", async () => {
    const stage = vi
      .fn()
      .mockImplementation(async (source: string, chatDir: string) =>
        source.replace("C:/source", `${chatDir}/attachments`)
      )

    const result = await preparePickedAttachmentPaths(
      ["C:/source/report.xlsx", "C:/source/notes.docx"],
      {
        stageInChatDirectory: true,
        chatDirectory: "F:/data/chat-sessions/2026-07-15/session",
        stage,
      }
    )

    expect(stage).toHaveBeenNthCalledWith(
      1,
      "C:/source/report.xlsx",
      "F:/data/chat-sessions/2026-07-15/session"
    )
    expect(stage).toHaveBeenNthCalledWith(
      2,
      "C:/source/notes.docx",
      "F:/data/chat-sessions/2026-07-15/session"
    )
    expect(result).toEqual([
      "F:/data/chat-sessions/2026-07-15/session/attachments/report.xlsx",
      "F:/data/chat-sessions/2026-07-15/session/attachments/notes.docx",
    ])
  })

  it("keeps original paths for repository sessions", async () => {
    const stage = vi.fn()
    const paths = ["C:/source/report.xlsx"]

    const result = await preparePickedAttachmentPaths(paths, {
      stageInChatDirectory: false,
      chatDirectory: "D:/repo",
      stage,
    })

    expect(result).toEqual(paths)
    expect(stage).not.toHaveBeenCalled()
  })
})
