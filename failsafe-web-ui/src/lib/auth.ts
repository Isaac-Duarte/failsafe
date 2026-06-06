const TOKEN_KEY = "failsafe_token"
const REFRESH_TOKEN_KEY = "failsafe_refresh_token"

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY)
}

export function getRefreshToken(): string | null {
  return localStorage.getItem(REFRESH_TOKEN_KEY)
}

export function setTokens(token: string, refreshToken: string): void {
  localStorage.setItem(TOKEN_KEY, token)
  localStorage.setItem(REFRESH_TOKEN_KEY, refreshToken)
}

export function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token)
}

export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY)
  localStorage.removeItem(REFRESH_TOKEN_KEY)
}

export function isAuthenticated(): boolean {
  return getToken() !== null
}
