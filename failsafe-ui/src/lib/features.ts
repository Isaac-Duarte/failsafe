import type { FeatureId } from "./bindings"

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
  {
    id: "screen_share",
    label: "Screen Share",
    description: "Allow other devices to view this screen",
  },
  {
    id: "port_forward",
    label: "Port Forward",
    description: "Accept forwarded TCP connections from other devices",
  },
] as const

export type KnownFeatureId = FeatureId

const featureById = new Map(KNOWN_FEATURES.map((feature) => [feature.id, feature]))

export function formatFeatureLabel(featureId: string): string {
  return featureById.get(featureId as KnownFeatureId)?.label ?? featureId
}

export function formatFeatureDescription(featureId: string): string | undefined {
  return featureById.get(featureId as KnownFeatureId)?.description
}

export function isKnownFeature(featureId: string): featureId is KnownFeatureId {
  return featureById.has(featureId as KnownFeatureId)
}

export function mergeEnabledFeatures(selected: FeatureId[]): FeatureId[] {
  const knownIds = new Set<FeatureId>(KNOWN_FEATURES.map((feature) => feature.id))
  const unknown = selected.filter((feature) => !knownIds.has(feature))
  const known = KNOWN_FEATURES.map((feature) => feature.id).filter((id) =>
    selected.includes(id)
  )
  return [...known, ...unknown]
}
