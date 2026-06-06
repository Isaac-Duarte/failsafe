import { useState } from "react"
import { toast } from "sonner"

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@failsafe/ui"
import { deleteDevice } from "@/lib/api"
import type { DeviceInfo } from "@failsafe/ui"

interface RemoveDeviceDialogProps {
  device: DeviceInfo | null
  onClose: () => void
  onRemoved: () => void
}

export function RemoveDeviceDialog({
  device,
  onClose,
  onRemoved,
}: RemoveDeviceDialogProps) {
  const [removeSaving, setRemoveSaving] = useState(false)

  async function handleConfirmRemove() {
    if (!device) {
      return
    }

    setRemoveSaving(true)

    try {
      await deleteDevice(device.device_id)
      toast.success("Device removed")
      onClose()
      onRemoved()
    } catch (err) {
      toast.error(
        err instanceof Error ? err.message : "Couldn't remove device"
      )
    } finally {
      setRemoveSaving(false)
    }
  }

  return (
    <AlertDialog
      open={device !== null}
      onOpenChange={(open) => {
        if (!open) {
          onClose()
        }
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remove device?</AlertDialogTitle>
          <AlertDialogDescription>
            {device ? (
              <>
                <span className="font-medium">{device.name}</span> will stop
                syncing with your other devices. Run{" "}
                <code className="rounded bg-muted px-1 py-0.5 text-xs">
                  failsafe pair --code &lt;CODE&gt;
                </code>{" "}
                on that machine to add it again.
              </>
            ) : null}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={removeSaving}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            className="bg-destructive text-white hover:bg-destructive/90"
            disabled={removeSaving}
            onClick={(event) => {
              event.preventDefault()
              void handleConfirmRemove()
            }}
          >
            {removeSaving ? "Removing..." : "Remove"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
