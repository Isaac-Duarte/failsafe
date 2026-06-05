import { useCallback, useEffect, useState } from "react"
import { useNavigate } from "react-router-dom"

import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { createPairingCode, listDevices } from "@/lib/api"
import { clearToken } from "@/lib/auth"
import type { DeviceInfo, PairingCreateResponse } from "@/lib/types"

function formatExpiry(expiresAt: string): string {
  const seconds = Math.max(0, Math.floor((new Date(expiresAt).getTime() - Date.now()) / 1000))
  const minutes = Math.floor(seconds / 60)
  const remainder = seconds % 60
  return `${minutes}:${remainder.toString().padStart(2, "0")}`
}

export function DevicesPage() {
  const navigate = useNavigate()
  const [devices, setDevices] = useState<DeviceInfo[]>([])
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [pairing, setPairing] = useState<PairingCreateResponse | null>(null)
  const [pairingLoading, setPairingLoading] = useState(false)
  const [copied, setCopied] = useState(false)
  const [expiryLabel, setExpiryLabel] = useState("")

  const loadDevices = useCallback(async () => {
    setError(null)
    setLoading(true)
    try {
      const response = await listDevices()
      setDevices(response.devices)
    } catch (err) {
      setError(err instanceof Error ? err.message : "failed to load devices")
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadDevices()
  }, [loadDevices])

  useEffect(() => {
    if (!pairing) {
      return
    }

    const updateExpiry = () => {
      if (new Date(pairing.expires_at) <= new Date()) {
        setExpiryLabel("Expired — generate a new code")
        return
      }
      setExpiryLabel(`Expires in ${formatExpiry(pairing.expires_at)}`)
    }

    updateExpiry()
    const timer = window.setInterval(updateExpiry, 1000)
    return () => window.clearInterval(timer)
  }, [pairing])

  async function handleCreatePairingCode() {
    setPairingLoading(true)
    setError(null)
    setCopied(false)

    try {
      const response = await createPairingCode()
      setPairing(response)
    } catch (err) {
      setError(err instanceof Error ? err.message : "failed to create pairing code")
    } finally {
      setPairingLoading(false)
    }
  }

  async function handleCopyCode() {
    if (!pairing) {
      return
    }

    await navigator.clipboard.writeText(pairing.code)
    setCopied(true)
    window.setTimeout(() => setCopied(false), 2000)
  }

  function handleLogout() {
    clearToken()
    navigate("/login", { replace: true })
  }

  return (
    <div className="mx-auto flex min-h-svh w-full max-w-5xl flex-col gap-6 p-6">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Devices</h1>
          <p className="text-sm text-muted-foreground">
            Manage paired devices and generate codes for new machines.
          </p>
        </div>
        <Button variant="outline" onClick={handleLogout}>
          Log out
        </Button>
      </div>

      {error ? (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle>Add a device</CardTitle>
          <CardDescription>
            Generate a pairing code, then run{" "}
            <code className="rounded bg-muted px-1 py-0.5 text-xs">
              failsafe pair --code &lt;CODE&gt;
            </code>{" "}
            on the new machine.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Button onClick={handleCreatePairingCode} disabled={pairingLoading}>
            {pairingLoading ? "Generating..." : "Add this device"}
          </Button>
          {pairing ? (
            <div className="space-y-2 rounded-lg border p-4">
              <p className="font-mono text-3xl font-bold tracking-[0.35em]">{pairing.code}</p>
              <p className="text-sm text-muted-foreground">{expiryLabel}</p>
              <Button variant="secondary" size="sm" onClick={handleCopyCode}>
                {copied ? "Copied" : "Copy code"}
              </Button>
            </div>
          ) : null}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Registered devices</CardTitle>
          <CardDescription>Devices linked to your account.</CardDescription>
        </CardHeader>
        <CardContent>
          {loading ? (
            <p className="text-sm text-muted-foreground">Loading devices...</p>
          ) : devices.length === 0 ? (
            <p className="text-sm text-muted-foreground">No devices registered yet.</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Device ID</TableHead>
                  <TableHead>Features</TableHead>
                  <TableHead>Last seen</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {devices.map((device) => (
                  <TableRow key={device.device_id}>
                    <TableCell className="font-medium">{device.name}</TableCell>
                    <TableCell className="font-mono text-xs">{device.device_id}</TableCell>
                    <TableCell>
                      <div className="flex flex-wrap gap-1">
                        {device.enabled_features.map((feature) => (
                          <Badge key={feature} variant="secondary">
                            {feature}
                          </Badge>
                        ))}
                      </div>
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">
                      {device.last_seen ? new Date(device.last_seen).toLocaleString() : "—"}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
