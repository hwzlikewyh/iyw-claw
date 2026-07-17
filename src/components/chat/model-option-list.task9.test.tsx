import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  type ReactNode,
} from "react"
import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

vi.mock("virtua", () => ({
  Virtualizer: forwardRef<
    { scrollToIndex: (index: number) => void },
    { children?: ReactNode }
  >(function VirtualizerMock(props, ref) {
    useImperativeHandle(ref, () => ({ scrollToIndex: vi.fn() }))
    return <>{props.children}</>
  }),
}))

vi.mock("@/components/ui/scroll-area", () => ({
  ScrollArea: ({
    children,
    onViewportRef,
  }: {
    children?: ReactNode
    onViewportRef?: (element: HTMLElement | null) => void
  }) => {
    useEffect(() => {
      onViewportRef?.(document.createElement("div"))
    }, [onViewportRef])
    return <div data-testid="model-scroll-area">{children}</div>
  },
}))

import { ModelOptionList } from "./model-option-list"

describe("ModelOptionList scrollbar host", () => {
  it("renders options inside the initialized internal scroll area", async () => {
    render(
      <ModelOptionList
        groups={[
          {
            key: "gateway",
            name: "gateway",
            options: [{ value: "gpt-5", name: "gpt-5", description: null }],
          },
        ]}
        currentValue="gpt-5"
        onSelect={vi.fn()}
        searchPlaceholder="Search"
        searchAriaLabel="Search"
        listAriaLabel="Models"
        emptyLabel="Empty"
      />
    )

    expect(await screen.findByTestId("model-scroll-area")).toBeTruthy()
    expect(screen.getByRole("option", { name: /gpt-5/ })).toBeTruthy()
  })
})
