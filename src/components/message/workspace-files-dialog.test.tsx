import "@testing-library/jest-dom/vitest"

import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { WorkspaceFilesDialog } from "./workspace-files-dialog"

const {
  getFileTree,
  openSettingsWindow,
  readFilePreview,
  readWorkspaceFileBase64,
  startOfficeWatch,
  stopOfficeWatch,
} = vi.hoisted(() => ({
  getFileTree: vi.fn(),
  openSettingsWindow: vi.fn(),
  readFilePreview: vi.fn(),
  readWorkspaceFileBase64: vi.fn(),
  startOfficeWatch: vi.fn(),
  stopOfficeWatch: vi.fn(),
}))

vi.mock("next-intl", () => ({
  useTranslations: () => (key: string) => key,
}))

vi.mock("@/contexts/active-folder-context", () => ({
  useActiveFolder: () => ({
    activeFolder: {
      id: 1,
      name: "iyw-claw",
      path: "D:/projects/iyw-claw",
      kind: "folder",
    },
  }),
}))

vi.mock("@/lib/api", () => ({
  getFileTree,
  openSettingsWindow,
  readFilePreview,
  readWorkspaceFileBase64,
  startOfficeWatch,
  stopOfficeWatch,
}))

vi.mock("@/lib/transport", () => ({
  getServerBaseUrl: () => "http://server.test",
  isDesktop: () => true,
  isRemoteDesktopMode: () => false,
}))

describe("WorkspaceFilesDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getFileTree.mockResolvedValue([
      {
        kind: "file",
        name: "report.docx",
        path: "report.docx",
      },
    ])
    readFilePreview.mockResolvedValue({
      path: "report.docx",
      content: "binary fallback",
    })
    startOfficeWatch.mockResolvedValue({ port: 26315, cap: "watch-cap" })
    stopOfficeWatch.mockResolvedValue(undefined)
  })

  it("previews Office files through OfficeCLI instead of the text reader", async () => {
    render(<WorkspaceFilesDialog />)
    fireEvent.click(screen.getByRole("button", { name: "open" }))

    fireEvent.click(await screen.findByText("report.docx"))

    await waitFor(() =>
      expect(startOfficeWatch).toHaveBeenCalledWith(
        "D:/projects/iyw-claw",
        "report.docx"
      )
    )
    expect(readFilePreview).not.toHaveBeenCalled()
    expect(await screen.findByTitle("officePreviewTitle")).toHaveAttribute(
      "src",
      "http://127.0.0.1:26315/"
    )
  })
})
