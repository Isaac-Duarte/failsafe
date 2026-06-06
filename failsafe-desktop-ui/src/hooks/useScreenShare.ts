import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { type RefObject, useCallback, useEffect, useState } from "react"

export function useScreenShare(
  deviceId: string | undefined,
  deviceName: string | undefined,
  viewportRef: RefObject<HTMLElement | null>
) {
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

  const syncViewport = useCallback(() => {
    const element = viewportRef.current
    if (!element) {
      return
    }

    const rect = element.getBoundingClientRect()
    const scale = window.devicePixelRatio || 1
    void invoke("set_screen_viewport", {
      bounds: {
        x: Math.round(rect.left * scale),
        y: Math.round(rect.top * scale),
        width: Math.round(rect.width * scale),
        height: Math.round(rect.height * scale),
      },
    }).catch(() => undefined)
  }, [viewportRef])

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
        syncViewport()
        await invoke("start_screen_share", {
          deviceId,
          deviceName: deviceName ?? deviceId,
        })
        if (!active) {
          return
        }
        setStatus("live")
        syncViewport()
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
  }, [deviceId, deviceName, syncViewport])

  useEffect(() => {
    syncViewport()

    const observer = new ResizeObserver(() => {
      syncViewport()
    })

    const element = viewportRef.current
    if (element) {
      observer.observe(element)
    }

    window.addEventListener("resize", syncViewport)

    return () => {
      observer.disconnect()
      window.removeEventListener("resize", syncViewport)
    }
  }, [syncViewport, viewportRef])

  return { status, error, stop }
}
