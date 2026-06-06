import { useEffect, useState } from "react"

import { getAccount } from "@/lib/api"
import { isAuthenticated } from "@/lib/auth"

export function useAccount() {
  const [email, setEmail] = useState<string | null>(null)
  const [loading, setLoading] = useState(isAuthenticated())

  useEffect(() => {
    if (!isAuthenticated()) {
      setEmail(null)
      setLoading(false)
      return
    }

    let cancelled = false
    setLoading(true)

    void getAccount()
      .then((account) => {
        if (!cancelled) {
          setEmail(account.email)
        }
      })
      .catch(() => {
        if (!cancelled) {
          setEmail(null)
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false)
        }
      })

    return () => {
      cancelled = true
    }
  }, [])

  return { email, loading }
}
