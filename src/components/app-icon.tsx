import Image from "next/image"
import { APP_LOGO } from "@/lib/brand"

export function AppIcon({ className }: { className?: string }) {
  return (
    <Image
      src={APP_LOGO}
      alt=""
      width={512}
      height={512}
      className={className}
      aria-hidden="true"
    />
  )
}
