import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

interface StatusBadgeProps {
  online: boolean
}

export function StatusBadge({ online }: StatusBadgeProps) {
  return (
    <Badge
      variant={online ? "default" : "secondary"}
      className={cn(
        "gap-1.5",
        online
          ? "bg-success text-success-foreground hover:bg-success/90"
          : "text-muted-foreground"
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full",
          online ? "bg-success-foreground" : "bg-muted-foreground"
        )}
      />
      {online ? "Online" : "Offline"}
    </Badge>
  )
}
