import { Link, Outlet, useLocation, useNavigate } from "react-router-dom"
import { ChevronDown, LogOut, Monitor, User } from "lucide-react"

import {
  AppShell,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Skeleton,
} from "@failsafe/ui"
import { useAccount } from "@/hooks/useAccount"
import { isAuthenticated } from "@/lib/auth"
import { logout } from "@/lib/api"

export function AppLayout() {
  const location = useLocation()
  const navigate = useNavigate()
  const authenticated = isAuthenticated()
  const { email, loading: accountLoading } = useAccount()
  const isLandingPage = location.pathname === "/"
  const isAuthPage =
    location.pathname === "/login" || location.pathname === "/register"
  const showSubtitle = isLandingPage || isAuthPage
  const homeHref = "/"

  async function handleLogout() {
    await logout()
    navigate("/login", { replace: true })
  }

  return (
    <AppShell
      homeHref={homeHref}
      subtitle={showSubtitle ? "Sync across your devices" : undefined}
      centered={isAuthPage}
      actions={
        !authenticated && isLandingPage ? (
          <>
            <Button asChild variant="outline" size="sm">
              <Link to="/login">Log in</Link>
            </Button>
            <Button asChild size="sm">
              <Link to="/register">Get started</Link>
            </Button>
          </>
        ) : authenticated ? (
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
              <DropdownMenuItem onClick={() => navigate("/account")}>
                <User />
                Account
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem variant="destructive" onClick={handleLogout}>
                <LogOut />
                Log out
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        ) : null
      }
    >
      <Outlet />
    </AppShell>
  )
}
