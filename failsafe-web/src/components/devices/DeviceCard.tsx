import { Pencil, Trash2 } from "lucide-react"

import { StatusBadge } from "@/components/StatusBadge"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { formatFeatureLabel } from "@/lib/features"
import { formatRelativeTime } from "@/lib/format"
import type { DeviceInfo } from "@/lib/types"

interface DeviceCardProps {
  device: DeviceInfo
  onEdit: (device: DeviceInfo) => void
  onRemove: (device: DeviceInfo) => void
}

export function DeviceCard({ device, onEdit, onRemove }: DeviceCardProps) {
  return (
    <div className="rounded-lg border bg-card p-4 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <p className="truncate font-medium">{device.name}</p>
          <StatusBadge online={device.online} />
        </div>
        <div className="flex shrink-0 gap-1">
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Edit ${device.name}`}
            onClick={() => onEdit(device)}
          >
            <Pencil />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Remove ${device.name}`}
            onClick={() => onRemove(device)}
          >
            <Trash2 />
          </Button>
        </div>
      </div>
      <div className="mt-3 space-y-2 text-sm">
        <div className="flex flex-wrap gap-1">
          {device.enabled_features.length === 0 ? (
            <span className="text-muted-foreground">No features enabled</span>
          ) : (
            device.enabled_features.map((feature) => (
              <Badge key={feature} variant="secondary">
                {formatFeatureLabel(feature)}
              </Badge>
            ))
          )}
        </div>
        <p className="text-muted-foreground">
          Last seen {formatRelativeTime(device.last_seen)}
        </p>
      </div>
    </div>
  )
}
