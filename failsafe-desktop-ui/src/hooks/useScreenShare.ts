import { Channel, invoke } from "@tauri-apps/api/core"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { useCallback, useEffect, useRef, useState } from "react"

export const SCREEN_QUALITY_PRESETS = [
  { value: "auto", label: "Auto" },
  { value: "1080p", label: "1080p" },
  { value: "720p", label: "720p" },
  { value: "480p", label: "480p" },
  { value: "360p", label: "360p" },
] as const

export type ScreenQualityPreset =
  (typeof SCREEN_QUALITY_PRESETS)[number]["value"]

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

function drawJpegToCanvas(
  canvas: HTMLCanvasElement,
  jpeg: Uint8Array
): Promise<void> {
  const blob = new Blob([jpeg], { type: "image/jpeg" })
  return createImageBitmap(blob).then((bitmap) => {
    if (canvas.width !== bitmap.width || canvas.height !== bitmap.height) {
      canvas.width = bitmap.width
      canvas.height = bitmap.height
    }
    const ctx = canvas.getContext("2d")
    if (ctx) {
      ctx.drawImage(bitmap, 0, 0)
    }
    bitmap.close()
  })
}

export function useScreenShare(
  deviceId: string | undefined,
  deviceName: string | undefined
) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const [hasFrame, setHasFrame] = useState(false)
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
    const unlisteners: UnlistenFn[] = []
    const initialQuality = loadStoredQuality()
    setQualityState(initialQuality)
    setFps(0)
    frameCountRef.current = 0
    setHasFrame(false)

    const frameChannel = new Channel<Uint8Array>()
    frameChannel.onmessage = (jpeg) => {
      const canvas = canvasRef.current
      if (!canvas) {
        return
      }
      void drawJpegToCanvas(canvas, jpeg).then(() => {
        setHasFrame(true)
        frameCountRef.current += 1
        setStatus("live")
      })
    }

    async function start() {
      setStatus("connecting")
      setError(null)

      try {
        await invoke("start_screen_share", {
          deviceId,
          deviceName: deviceName ?? deviceId,
          quality: initialQuality,
          onFrame: frameChannel,
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

  return { canvasRef, hasFrame, status, error, quality, fps, setQuality, stop }
}
