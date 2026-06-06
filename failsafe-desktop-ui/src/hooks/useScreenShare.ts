import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { useCallback, useEffect, useState } from "react"

export function useScreenShare(deviceId: string | undefined, deviceName: string | undefined) {
  const [status, setStatus] = useState<"idle" | "connecting" | "live" | "error" | "stopped">(
    "idle"
  )
  const [error, setError] = useState<string | null>(null)

  const stop = useCallback(async () => {
    try {
      await invoke("stop_screen_share")
    } catch {
      // Session may already be closed.
    }
    setStatus("stopped")
  }, [])

  useEffect(() => {
    if (!deviceId) {
      return
    }

    let active = true
    const unlisteners: UnlistenFn[] = []

    async function start() {
      setStatus("connecting")
      setError(null)

      try {
        await invoke("start_screen_share", {
          deviceId,
          deviceName: deviceName ?? deviceId,
        })
        if (!active) {
          return
        }
        setStatus("live")
      } catch (startError) {
        if (!active) {
          return
        }
        setStatus("error")
        setError(
          startError instanceof Error ? startError.message : String(startError)
        )
      }
    }

    async function bindListeners() {
      unlisteners.push(
        await listen<string>("screen-error", (event) => {
          setStatus("error")
          setError(event.payload)
        })
      )
      unlisteners.push(
        await listen("screen-stopped", () => {
          setStatus("stopped")
        })
      )
    }

    void bindListeners().then(() => start())

    return () => {
      active = false
      for (const unlisten of unlisteners) {
        unlisten()
      }
      void invoke("stop_screen_share").catch(() => undefined)
    }
  }, [deviceId, deviceName])

  return { status, error, stop }
}
