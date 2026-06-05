export interface AuthRegisterRequest {
  email: string
  password: string
}

export interface AuthLoginRequest {
  email: string
  password: string
}

export interface AuthResponse {
  token: string
}

export interface DeviceInfo {
  device_id: string
  name: string
  iroh_public_key: string
  enabled_features: string[]
  last_seen: string | null
  online: boolean
}

export interface DeviceListResponse {
  devices: DeviceInfo[]
}

export interface DevicePatchRequest {
  name?: string
  enabled_features?: string[]
}

export interface PairingCreateResponse {
  code: string
  expires_at: string
}

export interface ApiError {
  error: string
}
