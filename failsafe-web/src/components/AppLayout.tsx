import { Link, Outlet, useLocation, useNavigate } from "react-router-dom"

import { ThemeToggle } from "@/components/ThemeToggle"
import { Button } from "@/components/ui/button"
import { clearToken, isAuthenticated } from "@/lib/auth"

export function AppLayout() {
  const location = useLocation()
  const navigate = useNavigate()
  const authenticated = isAuthenticated()
  const isAuthPage = location.pathname === "/login" || location.pathname === "/register"
  const homeHref = authenticated ? "/devices" : "/login"

  function handleLogout() {
    clearToken()
    navigate("/login", { replace: true })
  }

  return (
    <div className="page-ambient flex min-h-svh flex-col">
      <header className="flex items-center justify-between gap-4 px-6 py-4">
        <Link
          to={homeHref}
          className="flex items-center gap-2.5 text-foreground transition-opacity hover:opacity-80"
        >
          <img src="/failsafe-logo.svg" alt="" className="size-8" />
          <span className="text-sm font-semibold tracking-tight">Failsafe</span>
        </Link>
        <div className="flex items-center gap-1">
          {authenticated ? (
            <Button variant="outline" size="sm" onClick={handleLogout}>
              Log out
            </Button>
          ) : null}
          <ThemeToggle />
        </div>
      </header>
      <main
        className={
          isAuthPage
            ? "flex flex-1 items-center justify-center px-6 pb-8"
            : "mx-auto w-full max-w-5xl flex-1 px-6 pb-8"
        }
      >
        <Outlet />
      </main>
    </div>
  )
}
