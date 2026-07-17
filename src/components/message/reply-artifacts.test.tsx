import { render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type { MessageTurn } from "@/lib/types"

import { ReplyArtifacts } from "./reply-artifacts"

vi.mock("next-intl", () => ({
  useTranslations: () => (key: string) => key,
}))

vi.mock("@/contexts/active-folder-context", () => ({
  useActiveFolder: () => ({ activeFolder: { path: "C:/workspace" } }),
}))

function assistantWriteTurn(): MessageTurn {
  return {
    id: "assistant-1",
    role: "assistant",
    timestamp: "2026-07-17T00:00:00Z",
    blocks: [
      {
        type: "tool_use",
        tool_use_id: "write-1",
        tool_name: "write",
        input_preview: JSON.stringify({
          file_path: "src/new-file.ts",
          content: "export const created = true\n",
        }),
      },
    ],
  }
}

function assistantEditTurn(): MessageTurn {
  return {
    id: "assistant-2",
    role: "assistant",
    timestamp: "2026-07-17T00:00:01Z",
    blocks: [
      {
        type: "tool_use",
        tool_use_id: "edit-1",
        tool_name: "edit",
        input_preview: JSON.stringify({
          file_path: "src/existing-file.ts",
          old_string: "export const value = 1",
          new_string: "export const value = 2",
        }),
      },
    ],
  }
}

describe("ReplyArtifacts", () => {
  it("does not render a completed reply summary for newly created files", () => {
    const { container } = render(
      <ReplyArtifacts sourceTurns={[assistantWriteTurn()]} isResponseComplete />
    )

    expect(container.firstChild).toBeNull()
  })

  it("keeps the completed reply summary for modified files", () => {
    const { getByText } = render(
      <ReplyArtifacts sourceTurns={[assistantEditTurn()]} isResponseComplete />
    )

    expect(getByText("title")).toBeTruthy()
  })
})
