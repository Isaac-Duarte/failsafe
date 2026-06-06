import type { ReactNode } from "react"

import { BrandHeader } from "./BrandHeader"

interface AppShellProps {
  homeHref: string
  subtitle?: string
  actions?: ReactNode
  centered?: boolean
  children: ReactNode
}

export function AppShell({
  homeHref,
  subtitle,
  actions,
  centered = false,
  children,
}: AppShellProps) {
  return (
    <div className="page-ambient flex min-h-svh flex-col">
      <BrandHeader homeHref={homeHref} subtitle={subtitle} actions={actions} />
      <main
        className={
          centered
            ? "flex flex-1 items-center justify-center px-6 pb-8"
            : "mx-auto w-full max-w-5xl flex-1 px-6 py-8"
        }
      >
        {children}
      </main>
    </div>
  )
}
