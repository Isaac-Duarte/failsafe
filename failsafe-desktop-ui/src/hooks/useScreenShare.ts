import type { ScreenFramePayload, ScreenQualityPreset } from "@failsafe/ui"
import { invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { useCallback, useEffect, useRef, useState } from "react"

export type { ScreenQualityPreset }

export const SCREEN_QUALITY_PRESETS: {
  value: ScreenQualityPreset
  label: string
}[] = [
  { value: "auto", label: "Auto" },
  { value: "1080p", label: "1080p" },
  { value: "720p", label: "720p" },
  { value: "480p", label: "480p" },
  { value: "360p", label: "360p" },
]

const QUALITY_STORAGE_KEY = "failsafe-screen-quality"

function loadStoredQuality(): ScreenQualityPreset {
  const stored = localStorage.getItem(QUALITY_STORAGE_KEY)
  if (
    stored &&
    SCREEN_QUALITY_PRESETS.some((preset) => preset.value === stored)
  ) {
    return stored as ScreenQualityPreset
  }
  return "auto"
}

export function useScreenShare(
  deviceId: string | undefined,
  deviceName: string | undefined
) {
  const [frameUrl, setFrameUrl] = useState<string | null>(null)
  const [status, setStatus] = useState<"idle" | "connecting" | "live" | "error" | "stopped">(
    "idle"
  )
  const [error, setError] = useState<string | null>(null)
  const [quality, setQualityState] = useState<ScreenQualityPreset>(loadStoredQuality)
  const [fps, setFps] = useState(0)
  const frameCountRef = useRef(0)

  const stop = useCallback(async () => {
    try {
      await invoke("stop_screen_share")
    } catch {
      // Session may already be closed.
    }
    setStatus("stopped")
  }, [])

  const setQuality = useCallback(
    async (preset: ScreenQualityPreset) => {
      setQualityState(preset)
      localStorage.setItem(QUALITY_STORAGE_KEY, preset)

      if (status === "connecting" || status === "live") {
        try {
          await invoke("set_screen_quality", { preset })
        } catch (qualityError) {
          setStatus("error")
          setError(
            qualityError instanceof Error
              ? qualityError.message
              : String(qualityError)
          )
        }
      }
    },
    [status]
  )

  useEffect(() => {
    const interval = window.setInterval(() => {
      setFps(frameCountRef.current)
      frameCountRef.current = 0
    }, 1000)

    return () => {
      window.clearInterval(interval)
    }
  }, [])

  useEffect(() => {
    if (!deviceId) {
      return
    }

    let active = true
    let objectUrl: string | null = null
    const unlisteners: UnlistenFn[] = []
    const initialQuality = loadStoredQuality()
    setQualityState(initialQuality)
    setFps(0)
    frameCountRef.current = 0

    async function start() {
      setStatus("connecting")
      setError(null)

      try {
        await invoke("start_screen_share", {
          deviceId,
          deviceName: deviceName ?? deviceId,
          quality: initialQuality,
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
        await listen<ScreenFramePayload>("screen-frame", (event) => {
          const bytes = new Uint8Array(event.payload.jpeg)
          const blob = new Blob([bytes], { type: "image/jpeg" })
          const nextUrl = URL.createObjectURL(blob)
          setFrameUrl((current) => {
            if (current) {
              URL.revokeObjectURL(current)
            }
            return nextUrl
          })
          objectUrl = nextUrl
          frameCountRef.current += 1
          setStatus("live")
        })
      )

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
      if (objectUrl) {
        URL.revokeObjectURL(objectUrl)
      }
      void invoke("stop_screen_share").catch(() => undefined)
    }
  }, [deviceId, deviceName])

  return { frameUrl, status, error, quality, fps, setQuality, stop }
}
