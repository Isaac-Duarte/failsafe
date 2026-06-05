import { clearToken, getToken } from "@/lib/auth"
import type {
  ApiError,
  AuthLoginRequest,
  AuthRegisterRequest,
  AuthResponse,
  DeviceListResponse,
  PairingCreateResponse,
} from "@/lib/types"

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
    if (response.status === 401) {
      clearToken()
    }
    throw new ApiRequestError(body.error ?? `request failed (${response.status})`, response.status)
  }

  return body
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

export async function listDevices(): Promise<DeviceListResponse> {
  const token = getToken()
  const response = await fetch("/api/v1/devices", {
    headers: token ? { authorization: `Bearer ${token}` } : {},
  })
  return parseResponse(response)
}

export async function createPairingCode(): Promise<PairingCreateResponse> {
  const token = getToken()
  const response = await fetch("/api/v1/pairing", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: "{}",
  })
  return parseResponse(response)
}
