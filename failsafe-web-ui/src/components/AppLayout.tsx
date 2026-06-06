import { Outlet, useLocation, useNavigate } from "react-router-dom"
import { ChevronDown, LogOut, Monitor } from "lucide-react"

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
  const isAuthPage =
    location.pathname === "/login" || location.pathname === "/register"
  const homeHref = authenticated ? "/devices" : "/login"

  async function handleLogout() {
    await logout()
    navigate("/login", { replace: true })
  }

  return (
    <AppShell
      homeHref={homeHref}
      subtitle={isAuthPage ? "Sync across your devices" : undefined}
      centered={isAuthPage}
      actions={
        authenticated ? (
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
        ) : null
      }
    >
      <Outlet />
    </AppShell>
  )
}
