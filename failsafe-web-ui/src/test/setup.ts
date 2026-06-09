const store = new Map<string, string>()

const localStorageMock: Storage = {
  get length() {
    return store.size
  },
  clear() {
    store.clear()
  },
  getItem(key) {
    return store.get(key) ?? null
  },
  key(index) {
    return Array.from(store.keys())[index] ?? null
  },
  removeItem(key) {
    store.delete(key)
  },
  setItem(key, value) {
    store.set(key, value)
  },
}

Object.defineProperty(globalThis, "localStorage", {
  configurable: true,
  value: localStorageMock,
})
