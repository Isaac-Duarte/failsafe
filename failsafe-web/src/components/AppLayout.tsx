import { Link, Outlet, useLocation, useNavigate } from "react-router-dom"
import { ChevronDown, LogOut, Monitor } from "lucide-react"

import { ThemeToggle } from "@/components/ThemeToggle"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Skeleton } from "@/components/ui/skeleton"
import { useAccount } from "@/hooks/useAccount"
import { clearToken, isAuthenticated } from "@/lib/auth"

export function AppLayout() {
  const location = useLocation()
  const navigate = useNavigate()
  const authenticated = isAuthenticated()
  const { email, loading: accountLoading } = useAccount()
  const isAuthPage =
    location.pathname === "/login" || location.pathname === "/register"
  const homeHref = authenticated ? "/devices" : "/login"

  function handleLogout() {
    clearToken()
    navigate("/login", { replace: true })
  }

  return (
    <div className="page-ambient flex min-h-svh flex-col">
      <header className="flex items-center justify-between gap-4 border-b border-border/50 px-6 py-4">
        <Link
          to={homeHref}
          className="flex items-center gap-3 text-foreground transition-opacity hover:opacity-80"
        >
          <img
            src="/failsafe-logo.svg"
            alt="Failsafe"
            className="size-10"
          />
          <div className="flex flex-col">
            <span className="text-base font-semibold tracking-tight">
              Failsafe
            </span>
            {isAuthPage ? (
              <span className="text-xs text-muted-foreground">
                Sync across your devices
              </span>
            ) : null}
          </div>
        </Link>
        <div className="flex items-center gap-1">
          {authenticated ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" size="sm" className="gap-1.5">
                  {accountLoading ? (
                    <Skeleton className="h-4 w-24" />
                  ) : (
                    <span className="max-w-[10rem] truncate">
                      {email ?? "Account"}
                    </span>
                  )}
                  <ChevronDown className="size-3.5 opacity-60" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-48">
                {email ? (
                  <>
                    <DropdownMenuLabel className="truncate font-normal text-muted-foreground">
                      {email}
                    </DropdownMenuLabel>
                    <DropdownMenuSeparator />
                  </>
                ) : null}
                <DropdownMenuItem onClick={() => navigate("/devices")}>
                  <Monitor />
                  Devices
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem variant="destructive" onClick={handleLogout}>
                  <LogOut />
                  Log out
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : null}
          <ThemeToggle />
        </div>
      </header>
      <main
        className={
          isAuthPage
            ? "flex flex-1 items-center justify-center px-6 pb-8"
            : "mx-auto w-full max-w-5xl flex-1 px-6 py-8"
        }
      >
        <Outlet />
      </main>
    </div>
  )
}
