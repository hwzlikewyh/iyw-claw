import { ChevronRight } from "lucide-react"

interface AssistantArchiveToggleProps {
  open: boolean
  contentId: string
  label: string
  onToggle: () => void
}

export function AssistantArchiveToggle(props: AssistantArchiveToggleProps) {
  return (
    <button
      type="button"
      className="conversation-fold-control"
      aria-expanded={props.open}
      aria-controls={props.contentId}
      onClick={props.onToggle}
    >
      {props.label}
      <ChevronRight
        aria-hidden="true"
        className={props.open ? "rotate-90" : undefined}
      />
    </button>
  )
}
