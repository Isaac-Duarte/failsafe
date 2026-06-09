import type { ReactNode } from "react";
import { Link } from "react-router-dom";

import { ThemeToggle } from "./ThemeToggle";
import logoUrl from "../assets/failsafe-logo.svg";

interface BrandHeaderProps {
  homeHref: string;
  subtitle?: string;
  actions?: ReactNode;
}

export function BrandHeader({ homeHref, subtitle, actions }: BrandHeaderProps) {
  return (
    <header className="sticky top-0 z-30 flex items-center justify-between gap-4 border-b border-border/55 bg-background/78 px-4 py-3 backdrop-blur-2xl sm:px-6">
      <Link
        to={homeHref}
        className="group flex min-w-0 items-center gap-3 text-foreground transition-opacity hover:opacity-85"
      >
        <span className="flex size-11 shrink-0 items-center justify-center rounded-xl border border-border/70 bg-card/80 shadow-sm">
          <img src={logoUrl} alt="Failsafe" className="size-8" />
        </span>
        <div className="flex min-w-0 flex-col">
          <span className="text-base font-semibold tracking-tight">
            Failsafe
          </span>
          {subtitle ? (
            <span className="truncate text-xs font-medium text-muted-foreground">
              {subtitle}
            </span>
          ) : null}
        </div>
      </Link>
      <div className="flex shrink-0 items-center gap-1 rounded-xl border border-border/50 bg-background/45 p-1 shadow-sm backdrop-blur">
        {actions}
        <ThemeToggle />
      </div>
    </header>
  );
}
