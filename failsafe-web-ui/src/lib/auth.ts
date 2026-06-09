const TOKEN_KEY = "failsafe_token"
const REFRESH_TOKEN_KEY = "failsafe_refresh_token"

export function getToken(): string | null {
  return globalThis.localStorage.getItem(TOKEN_KEY)
}

export function getRefreshToken(): string | null {
  return globalThis.localStorage.getItem(REFRESH_TOKEN_KEY)
}

export function setTokens(token: string, refreshToken: string): void {
  globalThis.localStorage.setItem(TOKEN_KEY, token)
  globalThis.localStorage.setItem(REFRESH_TOKEN_KEY, refreshToken)
}

export function setToken(token: string): void {
  globalThis.localStorage.setItem(TOKEN_KEY, token)
}

export function clearToken(): void {
  globalThis.localStorage.removeItem(TOKEN_KEY)
  globalThis.localStorage.removeItem(REFRESH_TOKEN_KEY)
}

export function isAuthenticated(): boolean {
  return getToken() !== null
}
