import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { WorkspaceFilesDialog } from "./workspace-files-dialog"

const { getFileTree, readFilePreview, readWorkspaceFileBase64 } = vi.hoisted(
  () => ({
    getFileTree: vi.fn(),
    readFilePreview: vi.fn(),
    readWorkspaceFileBase64: vi.fn(),
  })
)

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
  readFilePreview,
  readWorkspaceFileBase64,
}))

describe("WorkspaceFilesDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getFileTree.mockResolvedValue([
      {
        kind: "dir",
        name: "src",
        path: "src",
        children: [
          { kind: "file", name: "app.ts", path: "src/app.ts" },
          { kind: "file", name: "logo.png", path: "src/logo.png" },
        ],
      },
      { kind: "file", name: "README.md", path: "README.md" },
    ])
    readFilePreview.mockResolvedValue({
      path: "README.md",
      content: "# Workspace readme",
    })
    readWorkspaceFileBase64.mockResolvedValue("aW1hZ2U=")
  })

  it("opens for the active workspace and loads its file tree", async () => {
    render(<WorkspaceFilesDialog />)

    fireEvent.click(screen.getByRole("button", { name: "open" }))

    expect(await screen.findByRole("dialog")).toBeInTheDocument()
    expect(screen.getByText("D:/projects/iyw-claw")).toBeInTheDocument()
    await waitFor(() =>
      expect(getFileTree).toHaveBeenCalledWith("D:/projects/iyw-claw")
    )
    expect(await screen.findByText("src")).toBeInTheDocument()
    expect(screen.getByText("README.md")).toBeInTheDocument()
  })

  it("previews a selected text file inside the dialog", async () => {
    render(<WorkspaceFilesDialog />)
    fireEvent.click(screen.getByRole("button", { name: "open" }))

    fireEvent.click(await screen.findByText("README.md"))

    await waitFor(() =>
      expect(readFilePreview).toHaveBeenCalledWith(
        "D:/projects/iyw-claw",
        "README.md"
      )
    )
    expect(await screen.findByText("# Workspace readme")).toBeInTheDocument()
  })

  it("expands folders and previews selected images", async () => {
    render(<WorkspaceFilesDialog />)
    fireEvent.click(screen.getByRole("button", { name: "open" }))

    fireEvent.click(await screen.findByText("src"))
    fireEvent.click(await screen.findByText("logo.png"))

    await waitFor(() =>
      expect(readWorkspaceFileBase64).toHaveBeenCalledWith(
        "D:/projects/iyw-claw",
        "src/logo.png"
      )
    )
    expect(
      await screen.findByRole("img", { name: "logo.png" })
    ).toHaveAttribute("src", "data:image/png;base64,aW1hZ2U=")
  })
})
