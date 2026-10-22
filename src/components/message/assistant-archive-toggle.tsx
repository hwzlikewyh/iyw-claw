import { ChevronDown } from "lucide-react"

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
      <ChevronDown
        aria-hidden="true"
        className={props.open ? "rotate-180" : undefined}
      />
      {props.label}
    </button>
  )
}
