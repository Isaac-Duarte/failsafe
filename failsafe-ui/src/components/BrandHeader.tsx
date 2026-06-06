import type { ReactNode } from "react"
import { Link } from "react-router-dom"

import { ThemeToggle } from "./ThemeToggle"
import logoUrl from "../assets/failsafe-logo.svg"

interface BrandHeaderProps {
  homeHref: string
  subtitle?: string
  actions?: ReactNode
}

export function BrandHeader({ homeHref, subtitle, actions }: BrandHeaderProps) {
  return (
    <header className="flex items-center justify-between gap-4 border-b border-border/50 px-6 py-4">
      <Link
        to={homeHref}
        className="flex items-center gap-3 text-foreground transition-opacity hover:opacity-80"
      >
        <img src={logoUrl} alt="Failsafe" className="size-10" />
        <div className="flex flex-col">
          <span className="text-base font-semibold tracking-tight">Failsafe</span>
          {subtitle ? (
            <span className="text-xs text-muted-foreground">{subtitle}</span>
          ) : null}
        </div>
      </Link>
      <div className="flex items-center gap-1">
        {actions}
        <ThemeToggle />
      </div>
    </header>
  )
}
