import { useMemo } from "react"
import { useParams, useSearchParams } from "react-router-dom"
import { Monitor, X } from "lucide-react"

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
} from "@failsafe/ui"

export function ScreenSharePage() {
  const { deviceId } = useParams()
  const [searchParams] = useSearchParams()
  const deviceName = searchParams.get("name") ?? deviceId ?? "Device"
  const { status, error, stop } = useScreenShare(deviceId, deviceName)

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
    <div className="desktop-screen-share">
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
          <div className="flex items-start justify-between gap-4 rounded-xl bg-background/90 p-4 backdrop-blur-sm">
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
            <Alert variant="destructive" className="bg-background/90 backdrop-blur-sm">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          ) : null}

          <Card className="bg-card/90 shadow-lg ring-1 ring-border/50 backdrop-blur-sm">
            <CardHeader>
              <CardTitle>Remote display</CardTitle>
              <CardDescription>
                View-only session from the paired device daemon.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div className="screen-viewport relative flex min-h-[24rem] items-center justify-center overflow-hidden rounded-lg border border-border/50">
                {status !== "live" ? (
                  <p className="rounded-md bg-background/80 px-3 py-2 text-sm text-muted-foreground backdrop-blur-sm">
                    {status === "connecting"
                      ? "Opening screen share session..."
                      : "Waiting for frames..."}
                  </p>
                ) : null}
              </div>
            </CardContent>
          </Card>
        </div>
      </AppShell>
    </div>
  )
}
