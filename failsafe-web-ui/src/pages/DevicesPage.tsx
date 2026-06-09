import { useState } from "react"
import { RefreshCw } from "lucide-react"

import { DeviceList } from "@/components/devices/DeviceList"
import { EditDeviceDialog } from "@/components/devices/EditDeviceDialog"
import { PairingCard } from "@/components/devices/PairingCard"
import { RemoveDeviceDialog } from "@/components/devices/RemoveDeviceDialog"
import { Alert, AlertDescription, Badge } from "@failsafe/ui"
import { Button } from "@failsafe/ui"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@failsafe/ui"
import { useDevices } from "@/hooks/useDevices"
import type { DeviceInfo } from "@failsafe/ui"

export function DevicesPage() {
  const { devices, initialLoading, refreshing, error, reload } = useDevices()
  const [editingDevice, setEditingDevice] = useState<DeviceInfo | null>(null)
  const [removingDevice, setRemovingDevice] = useState<DeviceInfo | null>(null)

  const deviceCountLabel =
    !initialLoading && devices.length > 0
      ? `${devices.length} device${devices.length === 1 ? "" : "s"} linked`
      : null

  return (
    <div className="flex w-full flex-col gap-6">
      <div className="flex flex-col gap-4 rounded-2xl border border-border/65 bg-background/55 p-5 backdrop-blur md:flex-row md:items-end md:justify-between">
        <div>
          <Badge variant="outline" className="mb-3">
            Device fleet
          </Badge>
          <h1 className="text-3xl font-semibold tracking-tight">Devices</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            {deviceCountLabel ??
              "Manage paired devices and generate codes for new machines."}
          </p>
        </div>
        <div className="flex items-center gap-2 font-mono text-xs text-muted-foreground">
          <span className="size-2 rounded-full bg-success" />
          <span>control plane ready</span>
        </div>
      </div>

      {error ? (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}

      <PairingCard />

      <Card>
        <CardHeader>
          <CardTitle>Registered devices</CardTitle>
          <CardDescription>
            Devices linked to your account. Feature toggles control clipboard
            sync, remote shell, port forwarding, and other shared
            capabilities.
          </CardDescription>
          <CardAction>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void reload()}
              disabled={refreshing}
            >
              <RefreshCw className={refreshing ? "animate-spin" : ""} />
              Refresh
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          <DeviceList
            devices={devices}
            initialLoading={initialLoading}
            skeletonRowCount={devices.length || 3}
            onEdit={setEditingDevice}
            onRemove={setRemovingDevice}
          />
        </CardContent>
      </Card>

      <EditDeviceDialog
        device={editingDevice}
        onClose={() => setEditingDevice(null)}
        onSaved={() => void reload()}
      />

      <RemoveDeviceDialog
        device={removingDevice}
        onClose={() => setRemovingDevice(null)}
        onRemoved={() => void reload()}
      />
    </div>
  )
}
