import { describe, expect, it } from "vitest"

import { extractArtifactRegistration } from "./current-reply-artifacts"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"

function toolCall(
  toolName: string,
  input: unknown,
  output?: unknown
): AdaptedContentPart {
  return {
    type: "tool-call",
    toolCallId: `${toolName}-1`,
    toolName,
    input: JSON.stringify(input),
    state: "output-available",
    output:
      output === undefined
        ? null
        : typeof output === "string"
          ? output
          : JSON.stringify(output),
  }
}

describe("extractArtifactRegistration", () => {
  it("recognizes direct artifact registration", () => {
    expect(
      extractArtifactRegistration([
        toolCall(
          "mcp__iyw__present_task_files",
          { files: ["F:/deliverable/report.pdf"] },
          { accepted: [{ path: "F:/deliverable/report.pdf" }], rejected: [] }
        ),
      ])
    ).toEqual({
      hasCall: true,
      rejected: false,
      references: ["F:/deliverable/report.pdf"],
    })
  })

  it("recognizes a gateway artifact capability and accepted paths", () => {
    expect(
      extractArtifactRegistration([
        toolCall(
          "invoke_iyw_capability",
          {
            capability_id: "iyw.artifacts.present.v1",
            arguments: { files: ["F:/deliverable/report.pdf"] },
          },
          { accepted: [{ path: "F:/deliverable/report.pdf" }], rejected: [] }
        ),
      ])
    ).toEqual({
      hasCall: true,
      rejected: false,
      references: ["F:/deliverable/report.pdf"],
    })
  })

  it("does not treat another gateway capability as artifact registration", () => {
    expect(
      extractArtifactRegistration([
        toolCall(
          "invoke_iyw_capability",
          {
            capability_id: "iyw.image.search.v1",
            arguments: { files: ["F:/not-an-artifact.txt"] },
          },
          { accepted: [{ path: "F:/not-an-artifact.txt" }] }
        ),
      ])
    ).toEqual({ hasCall: false, rejected: false, references: [] })
  })

  it("suppresses input fallback for a failed or zero-success registration", () => {
    expect(
      extractArtifactRegistration([
        toolCall(
          "invoke_iyw_capability",
          {
            capability_id: "iyw.artifacts.present.v1",
            arguments: { files: ["F:/deliverable/report.pdf"] },
          },
          { accepted: [], rejected: [{ path: "F:/deliverable/report.pdf" }] }
        ),
      ])
    ).toEqual({ hasCall: true, rejected: true, references: [] })

    expect(
      extractArtifactRegistration([
        toolCall(
          "mcp__iyw__invoke_iyw_capability",
          {
            capability_id: "iyw.artifacts.present.v1",
            arguments: { files: ["F:/deliverable/report.pdf"] },
          },
          'Wall time: 0.01 seconds\nOutput:\n{"accepted":[],"rejected":[{"path":"F:/deliverable/report.pdf"}]}'
        ),
      ])
    ).toEqual({ hasCall: true, rejected: true, references: [] })

    expect(
      extractArtifactRegistration([
        toolCall(
          "present_task_files",
          { files: ["F:/deliverable/report.pdf"] },
          "Presented 0 task artifact(s); rejected 1."
        ),
      ])
    ).toEqual({ hasCall: true, rejected: true, references: [] })
  })
})
