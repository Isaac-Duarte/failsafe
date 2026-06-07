import type { FeatureId, FeatureInfo } from "./bindings"

export type KnownFeatureId = FeatureId

export function featureMap(features: FeatureInfo[]): Map<string, FeatureInfo> {
  return new Map(features.map((feature) => [feature.id, feature]))
}

export function formatFeatureLabel(featureId: string, features: FeatureInfo[]): string {
  return featureMap(features).get(featureId)?.label ?? featureId
}

export function formatFeatureDescription(
  featureId: string,
  features: FeatureInfo[]
): string | undefined {
  return featureMap(features).get(featureId)?.description
}

export function mergeEnabledFeatures(
  selected: FeatureId[],
  catalog: FeatureInfo[]
): FeatureId[] {
  const knownIds = new Set(catalog.map((feature) => feature.id))
  const unknown = selected.filter((feature) => !knownIds.has(feature))
  const known = catalog.map((feature) => feature.id).filter((id) => selected.includes(id))
  return [...known, ...unknown]
}
