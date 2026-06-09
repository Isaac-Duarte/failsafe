import { Badge } from "./ui/badge";
import { cn } from "../lib/utils";

interface StatusBadgeProps {
  online: boolean;
}

export function StatusBadge({ online }: StatusBadgeProps) {
  return (
    <Badge
      variant={online ? "default" : "secondary"}
      className={cn(
        "gap-1.5",
        online
          ? "bg-success/15 text-success ring-1 ring-success/25 hover:bg-success/20"
          : "text-muted-foreground",
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full",
          online
            ? "bg-success shadow-[0_0_12px_var(--success)]"
            : "bg-muted-foreground",
        )}
      />
      {online ? "Online" : "Offline"}
    </Badge>
  );
}
