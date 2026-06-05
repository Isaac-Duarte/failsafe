import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import type { DeviceInfo } from "@/lib/types"

import { DeviceCard } from "./DeviceCard"
import { DeviceTable } from "./DeviceTable"
import { EmptyDevices } from "./EmptyDevices"

const INITIAL_SKELETON_ROWS = 3

function DeviceTableSkeletonRow() {
  return (
    <TableRow>
      <TableCell>
        <Skeleton className="h-4 w-28" />
      </TableCell>
      <TableCell>
        <Skeleton className="h-5 w-16 rounded-4xl" />
      </TableCell>
      <TableCell className="hidden lg:table-cell">
        <Skeleton className="h-4 w-36" />
      </TableCell>
      <TableCell>
        <div className="flex gap-1">
          <Skeleton className="h-5 w-16 rounded-4xl" />
        </div>
      </TableCell>
      <TableCell>
        <Skeleton className="h-4 w-32" />
      </TableCell>
      <TableCell className="text-right">
        <div className="flex justify-end gap-1">
          <Skeleton className="size-8 rounded-md" />
          <Skeleton className="size-8 rounded-md" />
        </div>
      </TableCell>
    </TableRow>
  )
}

interface DeviceListProps {
  devices: DeviceInfo[]
  initialLoading: boolean
  skeletonRowCount?: number
  onEdit: (device: DeviceInfo) => void
  onRemove: (device: DeviceInfo) => void
}

export function DeviceList({
  devices,
  initialLoading,
  skeletonRowCount = INITIAL_SKELETON_ROWS,
  onEdit,
  onRemove,
}: DeviceListProps) {
  if (initialLoading) {
    return (
      <>
        <div className="hidden md:block">
          <Table aria-busy="true" aria-label="Loading devices">
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
              {Array.from({ length: skeletonRowCount }, (_, index) => (
                <DeviceTableSkeletonRow key={index} />
              ))}
            </TableBody>
          </Table>
        </div>
        <div className="flex flex-col gap-3 md:hidden">
          {Array.from({ length: skeletonRowCount }, (_, index) => (
            <Skeleton key={index} className="h-32 w-full rounded-lg" />
          ))}
        </div>
      </>
    )
  }

  if (devices.length === 0) {
    return <EmptyDevices />
  }

  return (
    <>
      <div className="hidden md:block">
        <DeviceTable devices={devices} onEdit={onEdit} onRemove={onRemove} />
      </div>
      <div className="flex flex-col gap-3 md:hidden">
        {devices.map((device) => (
          <DeviceCard
            key={device.device_id}
            device={device}
            onEdit={onEdit}
            onRemove={onRemove}
          />
        ))}
      </div>
    </>
  )
}
