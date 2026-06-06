import { Pencil, Trash2 } from "lucide-react"

import { StatusBadge } from "@failsafe/ui"
import { Badge } from "@failsafe/ui"
import { Button } from "@failsafe/ui"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@failsafe/ui"
import { formatFeatureLabel } from "@failsafe/ui"
import { formatRelativeTime } from "@failsafe/ui"
import type { DeviceInfo } from "@failsafe/ui"

interface DeviceTableProps {
  devices: DeviceInfo[]
  onEdit: (device: DeviceInfo) => void
  onRemove: (device: DeviceInfo) => void
}

export function DeviceTable({ devices, onEdit, onRemove }: DeviceTableProps) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Name</TableHead>
          <TableHead>Status</TableHead>
          <TableHead className="hidden lg:table-cell">Device ID</TableHead>
          <TableHead>Features</TableHead>
          <TableHead>Last seen</TableHead>
          <TableHead className="text-right">Actions</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {devices.map((device) => (
          <TableRow key={device.device_id}>
            <TableCell className="font-medium">{device.name}</TableCell>
            <TableCell>
              <StatusBadge online={device.online} />
            </TableCell>
            <TableCell className="hidden max-w-[12rem] truncate font-mono text-xs lg:table-cell">
              {device.device_id}
            </TableCell>
            <TableCell>
              <div className="flex flex-wrap gap-1">
                {device.enabled_features.length === 0 ? (
                  <span className="text-sm text-muted-foreground">none</span>
                ) : (
                  device.enabled_features.map((feature) => (
                    <Badge key={feature} variant="secondary">
                      {formatFeatureLabel(feature)}
                    </Badge>
                  ))
                )}
              </div>
            </TableCell>
            <TableCell className="text-sm text-muted-foreground">
              {formatRelativeTime(device.last_seen)}
            </TableCell>
            <TableCell className="text-right">
              <div className="flex justify-end gap-1">
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
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  )
}
