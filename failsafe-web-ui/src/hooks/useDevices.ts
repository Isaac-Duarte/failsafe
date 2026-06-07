import { useCallback, useEffect, useRef, useState } from "react"

import { listDevices } from "@/lib/api"
import type { DeviceInfo } from "@failsafe/ui"

const POLL_INTERVAL_MS = 30_000

export function useDevices() {
  const [devices, setDevices] = useState<DeviceInfo[]>([])
  const [initialLoading, setInitialLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const hasLoaded = useRef(false)

  const reload = useCallback(async () => {
    const isBackground = hasLoaded.current
    if (isBackground) {
      setRefreshing(true)
    }
    setError(null)

    try {
      const response = await listDevices()
      setDevices(response.devices)
      hasLoaded.current = true
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Couldn't load devices"
      )
    } finally {
      setInitialLoading(false)
      if (isBackground) {
        setRefreshing(false)
      }
    }
  }, [])

  useEffect(() => {
    let cancelled = false

    void (async () => {
      setError(null)

      try {
        const response = await listDevices()
        if (!cancelled) {
          setDevices(response.devices)
          hasLoaded.current = true
        }
      } catch (err) {
        if (!cancelled) {
          setError(
            err instanceof Error ? err.message : "Couldn't load devices"
          )
        }
      } finally {
        if (!cancelled) {
          setInitialLoading(false)
        }
      }
    })()

    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (document.visibilityState === "hidden") {
        return
      }
      void reload()
    }, POLL_INTERVAL_MS)

    return () => window.clearInterval(timer)
  }, [reload])

  return { devices, initialLoading, refreshing, error, reload }
}
