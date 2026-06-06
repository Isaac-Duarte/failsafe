import { useMemo } from "react"
import { useParams, useSearchParams } from "react-router-dom"
import { Check, Maximize2, Minimize2, Monitor, Settings2, X } from "lucide-react"

import { useFullscreen } from "@/hooks/useFullscreen"
import {
  SCREEN_QUALITY_PRESETS,
  useScreenShare,
  type ScreenQualityPreset,
} from "@/hooks/useScreenShare"
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
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@failsafe/ui"

export function ScreenSharePage() {
  const { deviceId } = useParams()
  const [searchParams] = useSearchParams()
  const deviceName = searchParams.get("name") ?? deviceId ?? "Device"
  const { isFullscreen, toggle: toggleFullscreen } = useFullscreen()
  const { frameUrl, status, error, quality, setQuality, stop } = useScreenShare(
    deviceId,
    deviceName
  )

  const qualityLabel = useMemo(
    () =>
      SCREEN_QUALITY_PRESETS.find((preset) => preset.value === quality)?.label ??
      "Auto",
    [quality]
  )

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

  function handleQualityChange(preset: ScreenQualityPreset) {
    void setQuality(preset)
  }

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
              className={cn(
                "screen-viewport relative flex items-center justify-center overflow-hidden",
                isFullscreen
                  ? "fixed inset-0 z-50 h-screen w-screen bg-black"
                  : "min-h-[24rem] rounded-lg border border-border/50"
              )}
              onDoubleClick={() => toggleFullscreen()}
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
                <div className="absolute top-3 right-3 flex items-center gap-2">
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        className="bg-background/80 backdrop-blur-sm"
                        onClick={(event) => event.stopPropagation()}
                      >
                        <Settings2 />
                        {qualityLabel}
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end">
                      <DropdownMenuLabel>Quality</DropdownMenuLabel>
                      {SCREEN_QUALITY_PRESETS.map((preset) => (
                        <DropdownMenuItem
                          key={preset.value}
                          onClick={() => handleQualityChange(preset.value)}
                        >
                          <span className="flex-1">{preset.label}</span>
                          {quality === preset.value ? (
                            <Check className="size-4" />
                          ) : null}
                        </DropdownMenuItem>
                      ))}
                    </DropdownMenuContent>
                  </DropdownMenu>
                  <Button
                    type="button"
                    variant="secondary"
                    size="icon-sm"
                    className="bg-background/80 backdrop-blur-sm"
                    onClick={(event) => {
                      event.stopPropagation()
                      toggleFullscreen()
                    }}
                    aria-label={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
                    title={
                      isFullscreen
                        ? "Exit fullscreen (Esc)"
                        : "Fullscreen (double-click stream)"
                    }
                  >
                    {isFullscreen ? <Minimize2 /> : <Maximize2 />}
                  </Button>
                </div>
              ) : null}
            </div>
          </CardContent>
        </Card>
      </div>
    </AppShell>
  )
}
