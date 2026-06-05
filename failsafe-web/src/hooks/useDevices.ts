import { useCallback, useEffect, useRef, useState } from "react"

import { listDevices } from "@/lib/api"
import type { DeviceInfo } from "@/lib/types"

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
    } else {
      setInitialLoading(true)
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
      setRefreshing(false)
    }
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

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
