import { afterEach, describe, expect, it } from "vitest"

import {
  clearToken,
  getRefreshToken,
  getToken,
  isAuthenticated,
  setTokens,
} from "./auth"

describe("auth token storage", () => {
  afterEach(() => {
    clearToken()
  })

  it("stores and reads access and refresh tokens", () => {
    setTokens("access-token", "refresh-token")
    expect(getToken()).toBe("access-token")
    expect(getRefreshToken()).toBe("refresh-token")
    expect(isAuthenticated()).toBe(true)
  })

  it("clears authentication state", () => {
    setTokens("access-token", "refresh-token")
    clearToken()
    expect(getToken()).toBeNull()
    expect(isAuthenticated()).toBe(false)
  })
})
