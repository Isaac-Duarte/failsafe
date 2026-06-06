import { useMemo, useRef } from "react"
import { useParams, useSearchParams } from "react-router-dom"
import { Maximize2, Minimize2, Monitor, X } from "lucide-react"

import { useFullscreen } from "@/hooks/useFullscreen"
import { useScreenShare } from "@/hooks/useScreenShare"
import {
  Alert,
  AlertDescription,
  AppShell,
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  cn,
} from "@failsafe/ui"

export function ScreenSharePage() {
  const { deviceId } = useParams()
  const [searchParams] = useSearchParams()
  const deviceName = searchParams.get("name") ?? deviceId ?? "Device"
  const viewportRef = useRef<HTMLDivElement>(null)
  const { isFullscreen, toggle: toggleFullscreen } = useFullscreen(viewportRef)
  const { frameUrl, status, error, stop } = useScreenShare(deviceId, deviceName)

  const statusLabel = useMemo(() => {
    switch (status) {
      case "connecting":
        return "Connecting"
      case "live":
        return "Live"
      case "error":
        return "Error"
      case "stopped":
        return "Stopped"
      default:
        return "Idle"
    }
  }, [status])

  return (
    <AppShell
      homeHref={`/screen-share/${deviceId ?? ""}`}
      subtitle="Screen share"
      actions={
        <Button variant="outline" size="sm" onClick={() => void stop()}>
          <X />
          Disconnect
        </Button>
      }
    >
      <div className="flex w-full flex-col gap-6">
        <div className="flex items-start justify-between gap-4">
          <div>
            <h1 className="text-2xl font-semibold tracking-tight">Screen share</h1>
            <p className="text-sm text-muted-foreground">
              Viewing <span className="font-medium text-foreground">{deviceName}</span>
            </p>
          </div>
          <Badge variant={status === "live" ? "default" : "secondary"} className="gap-1.5">
            <Monitor className="size-3.5" />
            {statusLabel}
          </Badge>
        </div>

        {error ? (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        ) : null}

        <Card className="shadow-lg ring-1 ring-border/50">
          <CardHeader>
            <CardTitle>Remote display</CardTitle>
            <CardDescription>
              View-only session from the paired device daemon. Double-click the stream to
              fullscreen.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div
              ref={viewportRef}
              className={cn(
                "screen-viewport relative flex items-center justify-center overflow-hidden",
                isFullscreen
                  ? "h-full w-full bg-black"
                  : "min-h-[24rem] rounded-lg border border-border/50"
              )}
              onDoubleClick={() => void toggleFullscreen()}
            >
              {frameUrl ? (
                <img
                  src={frameUrl}
                  alt={`Screen from ${deviceName}`}
                  className={cn(
                    "object-contain",
                    isFullscreen ? "h-full w-full" : "max-h-[70vh] w-full"
                  )}
                  draggable={false}
                />
              ) : null}
              {status !== "live" ? (
                <p
                  className={cn(
                    "rounded-md px-3 py-2 text-sm text-muted-foreground",
                    isFullscreen ? "bg-black/60 text-white" : "bg-muted"
                  )}
                >
                  {status === "connecting"
                    ? "Opening screen share session..."
                    : "Waiting for frames..."}
                </p>
              ) : null}
              {frameUrl ? (
                <Button
                  type="button"
                  variant="secondary"
                  size="icon-sm"
                  className="absolute top-3 right-3 bg-background/80 backdrop-blur-sm"
                  onClick={() => void toggleFullscreen()}
                  aria-label={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
                  title={isFullscreen ? "Exit fullscreen (Esc)" : "Fullscreen (double-click stream)"}
                >
                  {isFullscreen ? <Minimize2 /> : <Maximize2 />}
                </Button>
              ) : null}
            </div>
          </CardContent>
        </Card>
      </div>
    </AppShell>
  )
}
