export const KNOWN_FEATURES = [
  {
    id: "clipboard",
    label: "Clipboard",
    description: "Sync clipboard content across devices",
  },
  {
    id: "shell",
    label: "Shell",
    description: "Accept remote terminal sessions from other devices",
  },
] as const

export type KnownFeatureId = (typeof KNOWN_FEATURES)[number]["id"]

const featureById = new Map(KNOWN_FEATURES.map((feature) => [feature.id, feature]))

export function formatFeatureLabel(featureId: string): string {
  return featureById.get(featureId as KnownFeatureId)?.label ?? featureId
}

export function isKnownFeature(featureId: string): featureId is KnownFeatureId {
  return featureById.has(featureId as KnownFeatureId)
}

export function mergeEnabledFeatures(selected: string[]): string[] {
  const knownIds = new Set<string>(KNOWN_FEATURES.map((feature) => feature.id))
  const unknown = selected.filter((feature) => !knownIds.has(feature))
  const known = KNOWN_FEATURES.map((feature) => feature.id).filter((id) =>
    selected.includes(id)
  )
  return [...known, ...unknown]
}
