import { Link } from "react-router-dom"

import { Button } from "@failsafe/ui"
import { isAuthenticated } from "@/lib/auth"

export function NotFoundPage() {
  const homeHref = isAuthenticated() ? "/devices" : "/login"
  const homeLabel = isAuthenticated() ? "Go to devices" : "Go to login"

  return (
    <div className="flex flex-col items-center gap-4 py-16 text-center">
      <p className="text-6xl font-semibold text-muted-foreground">404</p>
      <div className="space-y-1">
        <h1 className="text-xl font-semibold">Page not found</h1>
        <p className="text-sm text-muted-foreground">
          The page you're looking for doesn't exist or has been moved.
        </p>
      </div>
      <Button asChild>
        <Link to={homeHref}>{homeLabel}</Link>
      </Button>
    </div>
  )
}
