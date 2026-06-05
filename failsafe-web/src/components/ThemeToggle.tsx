import { Monitor, Moon, Sun } from "lucide-react"

import { Button } from "@/components/ui/button"
import { useTheme } from "@/components/theme-provider"

const THEME_CYCLE = ["light", "dark", "system"] as const

function resolveIsDark(theme: string): boolean {
  if (theme === "dark") {
    return true
  }
  if (theme === "light") {
    return false
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches
}

export function ThemeToggle() {
  const { theme, setTheme } = useTheme()
  const isDark = resolveIsDark(theme)

  function cycleTheme() {
    const currentIndex = THEME_CYCLE.indexOf(theme as (typeof THEME_CYCLE)[number])
    const nextIndex =
      currentIndex === -1 ? 0 : (currentIndex + 1) % THEME_CYCLE.length
    setTheme(THEME_CYCLE[nextIndex])
  }

  const label =
    theme === "system"
      ? "System theme"
      : isDark
        ? "Switch to light mode"
        : "Switch to dark mode"

  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={cycleTheme}
      aria-label={label}
      title={label}
    >
      {theme === "system" ? (
        <Monitor />
      ) : isDark ? (
        <Sun />
      ) : (
        <Moon />
      )}
    </Button>
  )
}
