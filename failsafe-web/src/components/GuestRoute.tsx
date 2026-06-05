import { Navigate } from "react-router-dom"

import { isAuthenticated } from "@/lib/auth"

export function GuestRoute({ children }: { children: React.ReactNode }) {
  if (isAuthenticated()) {
    return <Navigate to="/devices" replace />
  }

  return children
}
