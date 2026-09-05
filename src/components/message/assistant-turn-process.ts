import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import {
  imageArtifactRegistrationState,
  type ImageArtifactRegistrationState,
} from "@/lib/image-delivery"

export type ImageRegistrationIssue = Extract<
  ImageArtifactRegistrationState,
  "failed" | "partial"
>

export function hasProcessError(part: AdaptedContentPart): boolean {
  if (part.type === "tool-call" || part.type === "tool-result") {
    return part.state === "output-error" || Boolean(part.errorText?.trim())
  }
  if (part.type === "generated-image") {
    const registration = imageArtifactRegistrationState(part.sourceToolOutput)
    return (
      part.status === "failed" ||
      (part.status === "completed" && part.image === null) ||
      registration === "failed" ||
      registration === "partial"
    )
  }
  if (part.type === "tool-group") return part.items.some(hasProcessError)
  if (part.type === "delegation-status-group")
    return part.polls.some(hasProcessError)
  if (part.type === "background-task-group")
    return part.polls.some(hasProcessError)
  if (part.type === "goal-run") {
    return (
      hasProcessError(part.start) ||
      Boolean(part.end && hasProcessError(part.end)) ||
      part.items.some(hasProcessError)
    )
  }
  return false
}

export function countProcessItems(parts: AdaptedContentPart[]): number {
  return parts.reduce((count, part) => {
    if (part.type === "tool-group") return count + part.items.length
    if (part.type === "delegation-status-group")
      return count + part.polls.length
    if (part.type === "background-task-group") return count + part.polls.length
    if (part.type === "goal-run") {
      return count + 1 + countProcessItems(part.items) + (part.end ? 1 : 0)
    }
    if (part.type === "text" && part.text.trim().length === 0) return count
    return count + 1
  }, 0)
}

export function findSummaryIndex(parts: AdaptedContentPart[]): number {
  for (let index = parts.length - 1; index >= 0; index -= 1) {
    const part = parts[index]
    if (part.type === "text" && part.text.trim().length > 0) return index
  }
  return -1
}

export function isFinalResultPart(part: AdaptedContentPart): boolean {
  return part.type === "displayed-image"
}

export function isLiveVisibleResultPart(part: AdaptedContentPart): boolean {
  return part.type === "generated-image" || isFinalResultPart(part)
}

export function isReasoningPart(part: AdaptedContentPart): boolean {
  return part.type === "reasoning"
}

export function completedProcessPart(
  part: AdaptedContentPart
): AdaptedContentPart | null {
  if (part.type === "generated-image") return null
  if (part.type !== "goal-run") return part
  return {
    ...part,
    items: part.items.flatMap((item) => {
      const visible = completedProcessPart(item)
      return visible ? [visible] : []
    }),
  }
}

export function findImageRegistrationIssue(
  parts: AdaptedContentPart[]
): ImageRegistrationIssue | null {
  let issue: ImageRegistrationIssue | null = null
  for (const part of parts) {
    const current = imageRegistrationIssue(part)
    if (current === "failed") return current
    if (current === "partial") issue = current
  }
  return issue
}

function imageRegistrationIssue(
  part: AdaptedContentPart
): ImageRegistrationIssue | null {
  if (part.type === "generated-image") {
    const state = imageArtifactRegistrationState(part.sourceToolOutput)
    return state === "failed" || state === "partial" ? state : null
  }
  if (part.type === "goal-run") {
    return findImageRegistrationIssue(part.items)
  }
  return null
}
