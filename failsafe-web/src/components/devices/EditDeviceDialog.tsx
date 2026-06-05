import { useEffect, useState } from "react"
import { toast } from "sonner"

import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { updateDevice } from "@/lib/api"
import type { DeviceInfo } from "@/lib/types"

const KNOWN_FEATURES = ["clipboard"] as const

interface EditDeviceDialogProps {
  device: DeviceInfo | null
  onClose: () => void
  onSaved: () => void
}

export function EditDeviceDialog({
  device,
  onClose,
  onSaved,
}: EditDeviceDialogProps) {
  const [editName, setEditName] = useState("")
  const [editFeatures, setEditFeatures] = useState<string[]>([])
  const [editSaving, setEditSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (device) {
      setEditName(device.name)
      setEditFeatures([...device.enabled_features])
      setError(null)
    }
  }, [device])

  function toggleEditFeature(feature: string, checked: boolean) {
    setEditFeatures((current) =>
      checked
        ? [...current, feature]
        : current.filter((item) => item !== feature)
    )
  }

  async function handleSaveEdit() {
    if (!device) {
      return
    }

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
        enabled_features: editFeatures,
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
    <Dialog
      open={device !== null}
      onOpenChange={(open) => {
        if (!open) {
          onClose()
          setError(null)
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit device</DialogTitle>
          <DialogDescription>
            Update the display name and which features this device can sync
            with others.
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
              Controls which features this device can sync with others.
            </p>
            <div className="space-y-2">
              {KNOWN_FEATURES.map((feature) => (
                <label
                  key={feature}
                  className="flex items-center gap-2 text-sm"
                >
                  <Checkbox
                    checked={editFeatures.includes(feature)}
                    onCheckedChange={(checked) =>
                      toggleEditFeature(feature, checked === true)
                    }
                    disabled={editSaving}
                  />
                  {feature}
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
      </DialogContent>
    </Dialog>
  )
}
