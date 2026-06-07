import { useEffect, useState } from "react"

import type { FeatureInfo } from "@failsafe/ui"
import { listFeatures } from "@/lib/api"

let cachedFeatures: FeatureInfo[] | null = null
let inflight: Promise<FeatureInfo[]> | null = null

async function loadFeatures(): Promise<FeatureInfo[]> {
  if (cachedFeatures) {
    return cachedFeatures
  }
  if (!inflight) {
    inflight = listFeatures()
      .then((response) => {
        cachedFeatures = response.features
        return cachedFeatures
      })
      .finally(() => {
        inflight = null
      })
  }
  return inflight
}

export function useFeatures() {
  const [features, setFeatures] = useState<FeatureInfo[]>(cachedFeatures ?? [])
  const [loading, setLoading] = useState(cachedFeatures === null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (cachedFeatures) {
      return
    }

    let cancelled = false
    void loadFeatures()
      .then((catalog) => {
        if (!cancelled) {
          setFeatures(catalog)
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : "Couldn't load features")
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false)
        }
      })

    return () => {
      cancelled = true
    }
  }, [])

  return { features, loading, error }
}
