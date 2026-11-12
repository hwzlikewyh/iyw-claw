import { render } from "@testing-library/react"
import { NextIntlClientProvider } from "next-intl"
import { expect, it } from "vitest"
import { MarketQuickPages } from "./market-quick-pages"

it("shows unavailable instead of a zero count after a failed catalog", () => {
  const messages = {
    MarketPagination: {
      loading: "Loading",
      previous: "Previous",
      next: "Next",
    },
    CapabilityMarket: {
      catalog: {
        unavailable: "Catalog unavailable",
        quickPages: "{total} items · {page} / {pages}",
      },
    },
  }
  const props = {
    page: 1,
    pageSize: 6,
    total: 0,
    loading: false,
    unavailable: true,
    onPageChange: () => {},
    onPageSizeChange: () => {},
  }
  const view = render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <MarketQuickPages {...props} />
    </NextIntlClientProvider>
  )
  expect(view.getByRole("status").textContent).toBe("Catalog unavailable")
  for (const button of view.getAllByRole("button"))
    expect((button as HTMLButtonElement).disabled).toBe(true)
  view.unmount()
})
