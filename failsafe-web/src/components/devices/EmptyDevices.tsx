import { Monitor } from "lucide-react"

export function EmptyDevices() {
  return (
    <div className="flex flex-col items-center gap-3 py-10 text-center">
      <div className="flex size-12 items-center justify-center rounded-full bg-muted">
        <Monitor className="size-6 text-muted-foreground" />
      </div>
      <div className="space-y-1">
        <p className="font-medium">No devices yet</p>
        <p className="max-w-sm text-sm text-muted-foreground">
          Generate a pairing code above, then run the CLI on your new machine to
          link it to your account.
        </p>
      </div>
    </div>
  )
}
