import { clearToken, getRefreshToken, getToken, setTokens } from "@/lib/auth"
import { emitUnauthorized } from "@/lib/auth-events"
import type {
  AccountResponse,
  ApiError,
  AuthLoginRequest,
  AuthLogoutRequest,
  AuthMfaLoginRequest,
  AuthRefreshRequest,
  AuthRegisterRequest,
  AuthResponse,
  ChangePasswordRequest,
  DeviceInfo,
  DeviceListResponse,
  DevicePatchRequest,
  PairingCreateResponse,
  TotpDisableRequest,
  TotpEnableRequest,
  TotpEnableResponse,
  TotpSetupResponse,
} from "@failsafe/ui"

export class ApiRequestError extends Error {
  status: number

  constructor(message: string, status: number) {
    super(message)
    this.status = status
  }
}

async function parseResponse<T>(response: Response): Promise<T> {
  const body = (await response.json().catch(() => ({}))) as T & ApiError

  if (!response.ok) {
    throw new ApiRequestError(body.error ?? `request failed (${response.status})`, response.status)
  }

  return body
}

async function refreshTokens(): Promise<boolean> {
  const refreshToken = getRefreshToken()
  if (!refreshToken) {
    return false
  }

  const response = await fetch("/api/v1/auth/refresh", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ refresh_token: refreshToken } satisfies AuthRefreshRequest),
  })

  if (!response.ok) {
    return false
  }

  const body = (await response.json()) as AuthResponse
  if (!body.token || !body.refresh_token) {
    return false
  }
  setTokens(body.token, body.refresh_token)
  return true
}

async function authFetch(url: string, init: RequestInit = {}): Promise<Response> {
  const token = getToken()
  const headers = new Headers(init.headers)
  if (token) {
    headers.set("authorization", `Bearer ${token}`)
  }

  let response = await fetch(url, { ...init, headers })

  if (response.status === 401 && (await refreshTokens())) {
    const refreshedToken = getToken()
    const retryHeaders = new Headers(init.headers)
    if (refreshedToken) {
      retryHeaders.set("authorization", `Bearer ${refreshedToken}`)
    }
    response = await fetch(url, { ...init, headers: retryHeaders })
  }

  if (response.status === 401) {
    clearToken()
    emitUnauthorized()
  }

  return response
}

export async function register(request: AuthRegisterRequest): Promise<AuthResponse> {
  const response = await fetch("/api/v1/auth/register", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  })
  return parseResponse(response)
}

export async function login(request: AuthLoginRequest): Promise<AuthResponse> {
  const response = await fetch("/api/v1/auth/login", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  })
  return parseResponse(response)
}

export async function loginMfa(request: AuthMfaLoginRequest): Promise<AuthResponse> {
  const response = await fetch("/api/v1/auth/login/mfa", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  })
  return parseResponse(response)
}

export async function changePassword(request: ChangePasswordRequest): Promise<void> {
  const response = await authFetch("/api/v1/auth/password", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  })
  await parseResponse(response)
}

export async function setupTotp(): Promise<TotpSetupResponse> {
  const response = await authFetch("/api/v1/auth/2fa/setup", {
    method: "POST",
  })
  return parseResponse(response)
}

export async function enableTotp(request: TotpEnableRequest): Promise<TotpEnableResponse> {
  const response = await authFetch("/api/v1/auth/2fa/enable", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  })
  return parseResponse(response)
}

export async function disableTotp(request: TotpDisableRequest): Promise<void> {
  const response = await authFetch("/api/v1/auth/2fa/disable", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  })
  await parseResponse(response)
}

export async function logout(): Promise<void> {
  const refreshToken = getRefreshToken()
  if (refreshToken) {
    await fetch("/api/v1/auth/logout", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ refresh_token: refreshToken } satisfies AuthLogoutRequest),
    }).catch(() => undefined)
  }
  clearToken()
}

export async function listDevices(): Promise<DeviceListResponse> {
  const response = await authFetch("/api/v1/devices")
  return parseResponse(response)
}

export async function updateDevice(
  deviceId: string,
  patch: DevicePatchRequest,
): Promise<DeviceInfo> {
  const response = await authFetch(`/api/v1/devices/${deviceId}`, {
    method: "PATCH",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(patch),
  })
  return parseResponse(response)
}

export async function deleteDevice(deviceId: string): Promise<void> {
  const response = await authFetch(`/api/v1/devices/${deviceId}`, {
    method: "DELETE",
  })
  await parseResponse(response)
}

export async function getAccount(): Promise<AccountResponse> {
  const response = await authFetch("/api/v1/auth/me")
  return parseResponse(response)
}

export async function createPairingCode(): Promise<PairingCreateResponse> {
  const response = await authFetch("/api/v1/pairing", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: "{}",
  })
  return parseResponse(response)
}
