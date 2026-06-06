import { useEffect } from "react"
import { useNavigate } from "react-router-dom"

import { onUnauthorized } from "@/lib/auth-events"

export function UnauthorizedHandler() {
  const navigate = useNavigate()

  useEffect(() => {
    return onUnauthorized(() => {
      navigate("/login?session=expired", { replace: true })
    })
  }, [navigate])

  return null
}
