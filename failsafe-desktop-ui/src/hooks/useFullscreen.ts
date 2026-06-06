import { useCallback, useEffect, useState, type RefObject } from "react"

function fullscreenElement(): Element | null {
  const doc = document as Document & { webkitFullscreenElement?: Element | null }
  return document.fullscreenElement ?? doc.webkitFullscreenElement ?? null
}

export function useFullscreen(targetRef: RefObject<HTMLElement | null>) {
  const [isFullscreen, setIsFullscreen] = useState(false)

  useEffect(() => {
    const onChange = () => {
      setIsFullscreen(fullscreenElement() === targetRef.current)
    }

    document.addEventListener("fullscreenchange", onChange)
    document.addEventListener("webkitfullscreenchange", onChange)
    return () => {
      document.removeEventListener("fullscreenchange", onChange)
      document.removeEventListener("webkitfullscreenchange", onChange)
    }
  }, [targetRef])

  const toggle = useCallback(async () => {
    const element = targetRef.current
    if (!element) {
      return
    }

    if (fullscreenElement()) {
      if (document.exitFullscreen) {
        await document.exitFullscreen()
      } else {
        const doc = document as Document & { webkitExitFullscreen?: () => Promise<void> }
        await doc.webkitExitFullscreen?.()
      }
      return
    }

    if (element.requestFullscreen) {
      await element.requestFullscreen()
      return
    }

    const webkitElement = element as HTMLElement & {
      webkitRequestFullscreen?: () => Promise<void>
    }
    await webkitElement.webkitRequestFullscreen?.()
  }, [targetRef])

  return { isFullscreen, toggle }
}
