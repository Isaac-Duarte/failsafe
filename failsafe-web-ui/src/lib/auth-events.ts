type UnauthorizedListener = () => void

let listener: UnauthorizedListener | null = null

export function onUnauthorized(callback: UnauthorizedListener): () => void {
  listener = callback
  return () => {
    if (listener === callback) {
      listener = null
    }
  }
}

export function emitUnauthorized(): void {
  listener?.()
}
