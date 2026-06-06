const SECOND = 1_000
const MINUTE = 60 * SECOND
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR

export function formatRelativeTime(iso: string | null): string {
  if (!iso) {
    return "—"
  }

  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) {
    return "—"
  }

  const delta = Date.now() - date.getTime()
  if (delta < MINUTE) {
    return "Just now"
  }
  if (delta < HOUR) {
    const minutes = Math.floor(delta / MINUTE)
    return `${minutes} min ago`
  }
  if (delta < DAY) {
    const hours = Math.floor(delta / HOUR)
    return `${hours} hr ago`
  }
  if (delta < DAY * 7) {
    const days = Math.floor(delta / DAY)
    return `${days} day${days === 1 ? "" : "s"} ago`
  }

  return date.toLocaleString()
}
