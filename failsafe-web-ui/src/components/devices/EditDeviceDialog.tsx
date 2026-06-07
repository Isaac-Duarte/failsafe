import { useState } from "react"
import { toast } from "sonner"

import { Alert, AlertDescription } from "@failsafe/ui"
import { Button } from "@failsafe/ui"
import { Checkbox } from "@failsafe/ui"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@failsafe/ui"
import { Input } from "@failsafe/ui"
import { Label } from "@failsafe/ui"
import { updateDevice } from "@/lib/api"
import { KNOWN_FEATURES, mergeEnabledFeatures } from "@failsafe/ui"
import type { DeviceInfo, FeatureId } from "@failsafe/ui"

interface EditDeviceDialogProps {
  device: DeviceInfo | null
  onClose: () => void
  onSaved: () => void
}

interface EditDeviceFormProps {
  device: DeviceInfo
  onClose: () => void
  onSaved: () => void
}

function EditDeviceForm({ device, onClose, onSaved }: EditDeviceFormProps) {
  const [editName, setEditName] = useState(device.name)
  const [editFeatures, setEditFeatures] = useState<FeatureId[]>([
    ...device.enabled_features,
  ])
  const [editSaving, setEditSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  function toggleEditFeature(feature: FeatureId, checked: boolean) {
    setEditFeatures((current) =>
      checked
        ? [...current, feature]
        : current.filter((item) => item !== feature)
    )
  }

  async function handleSaveEdit() {
    const name = editName.trim()
    if (!name) {
      setError("Device name cannot be empty")
      return
    }

    setEditSaving(true)
    setError(null)

    try {
      await updateDevice(device.device_id, {
        name,
        enabled_features: mergeEnabledFeatures(editFeatures),
      })
      toast.success("Device updated")
      onClose()
      onSaved()
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Couldn't update device"
      )
    } finally {
      setEditSaving(false)
    }
  }

  return (
    <>
      <DialogHeader>
        <DialogTitle>Edit device</DialogTitle>
        <DialogDescription>
          Update the display name and which capabilities this device shares with
          others.
        </DialogDescription>
      </DialogHeader>
      <div className="space-y-4">
        {error ? (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        ) : null}
        <div className="space-y-2">
          <Label htmlFor="device-name">Name</Label>
          <Input
            id="device-name"
            value={editName}
            onChange={(event) => setEditName(event.target.value)}
            disabled={editSaving}
          />
        </div>
        <div className="space-y-2">
          <Label>Features</Label>
          <p className="text-xs text-muted-foreground">
            Both devices need a feature enabled for it to work between them.
          </p>
          <div className="space-y-3">
            {KNOWN_FEATURES.map((feature) => (
              <label
                key={feature.id}
                className="flex items-start gap-2 text-sm"
              >
                <Checkbox
                  className="mt-0.5"
                  checked={editFeatures.includes(feature.id)}
                  onCheckedChange={(checked) =>
                    toggleEditFeature(feature.id, checked === true)
                  }
                  disabled={editSaving}
                />
                <span className="space-y-0.5">
                  <span className="block font-medium">{feature.label}</span>
                  <span className="block text-xs text-muted-foreground">
                    {feature.description}
                  </span>
                </span>
              </label>
            ))}
          </div>
        </div>
      </div>
      <DialogFooter>
        <Button variant="outline" onClick={onClose} disabled={editSaving}>
          Cancel
        </Button>
        <Button onClick={() => void handleSaveEdit()} disabled={editSaving}>
          {editSaving ? "Saving..." : "Save"}
        </Button>
      </DialogFooter>
    </>
  )
}

export function EditDeviceDialog({
  device,
  onClose,
  onSaved,
}: EditDeviceDialogProps) {
  return (
    <Dialog
      open={device !== null}
      onOpenChange={(open) => {
        if (!open) {
          onClose()
        }
      }}
    >
      <DialogContent>
        {device ? (
          <EditDeviceForm
            key={device.device_id}
            device={device}
            onClose={onClose}
            onSaved={onSaved}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  )
}
