import { useCallback, useEffect, useState } from "react"

import { getAccount } from "@/lib/api"
import { isAuthenticated } from "@/lib/auth"

export function useAccount() {
  const [email, setEmail] = useState<string | null>(null)
  const [totpEnabled, setTotpEnabled] = useState(false)
  const [loading, setLoading] = useState(isAuthenticated())

  const refresh = useCallback(async () => {
    if (!isAuthenticated()) {
      setEmail(null)
      setTotpEnabled(false)
      setLoading(false)
      return
    }

    setLoading(true)
    try {
      const account = await getAccount()
      setEmail(account.email)
      setTotpEnabled(account.totp_enabled)
    } catch {
      setEmail(null)
      setTotpEnabled(false)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    let cancelled = false

    void (async () => {
      if (!isAuthenticated()) {
        if (!cancelled) {
          setEmail(null)
          setTotpEnabled(false)
          setLoading(false)
        }
        return
      }

      if (!cancelled) {
        setLoading(true)
      }

      try {
        const account = await getAccount()
        if (!cancelled) {
          setEmail(account.email)
          setTotpEnabled(account.totp_enabled)
        }
      } catch {
        if (!cancelled) {
          setEmail(null)
          setTotpEnabled(false)
        }
      } finally {
        if (!cancelled) {
          setLoading(false)
        }
      }
    })()

    return () => {
      cancelled = true
    }
  }, [])

  return { email, totpEnabled, loading, refresh }
}
