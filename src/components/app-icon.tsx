import Image from "next/image"

export function AppIcon({ className }: { className?: string }) {
  return (
    <Image
      src="/icon.svg"
      alt=""
      width={512}
      height={512}
      className={className}
      aria-hidden="true"
    />
  )
}
