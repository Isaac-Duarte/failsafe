import { Monitor } from "lucide-react"

export function EmptyDevices() {
  return (
    <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-border/80 bg-muted/30 py-12 text-center">
      <div className="flex size-14 items-center justify-center rounded-xl border border-border/70 bg-background/65">
        <Monitor className="size-6 text-primary" />
      </div>
      <div className="space-y-1">
        <p className="font-semibold tracking-tight">No devices in the fleet</p>
        <p className="max-w-sm text-sm text-muted-foreground">
          Generate a pairing code above, then run the CLI on your new machine to
          link it to your account.
        </p>
      </div>
    </div>
  )
}
