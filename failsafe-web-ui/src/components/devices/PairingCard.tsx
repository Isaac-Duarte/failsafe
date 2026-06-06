import { useEffect, useState } from "react"
import { Check, Copy } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@failsafe/ui"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@failsafe/ui"
import { createPairingCode } from "@/lib/api"
import type { PairingCreateResponse } from "@failsafe/ui"

function formatExpiry(expiresAt: string): string {
  const seconds = Math.max(
    0,
    Math.floor((new Date(expiresAt).getTime() - Date.now()) / 1000)
  )
  const minutes = Math.floor(seconds / 60)
  const remainder = seconds % 60
  return `${minutes}:${remainder.toString().padStart(2, "0")}`
}

function isExpired(expiresAt: string): boolean {
  return new Date(expiresAt) <= new Date()
}

export function PairingCard() {
  const [pairing, setPairing] = useState<PairingCreateResponse | null>(null)
  const [pairingLoading, setPairingLoading] = useState(false)
  const [copied, setCopied] = useState(false)
  const [expiryLabel, setExpiryLabel] = useState("")
  const [expired, setExpired] = useState(false)

  useEffect(() => {
    if (!pairing) {
      return
    }

    const updateExpiry = () => {
      if (isExpired(pairing.expires_at)) {
        setExpiryLabel("Expired — generate a new code")
        setExpired(true)
        return
      }
      setExpired(false)
      setExpiryLabel(`Expires in ${formatExpiry(pairing.expires_at)}`)
    }

    updateExpiry()
    const timer = window.setInterval(updateExpiry, 1000)
    return () => window.clearInterval(timer)
  }, [pairing])

  async function handleCreatePairingCode() {
    setPairingLoading(true)
    setCopied(false)

    try {
      const response = await createPairingCode()
      setPairing(response)
      setExpired(false)
    } catch (err) {
      toast.error(
        err instanceof Error ? err.message : "Couldn't create pairing code"
      )
    } finally {
      setPairingLoading(false)
    }
  }

  async function handleCopyCode() {
    if (!pairing || expired) {
      return
    }

    try {
      await navigator.clipboard.writeText(pairing.code)
      setCopied(true)
      toast.success("Pairing code copied")
      window.setTimeout(() => setCopied(false), 2000)
    } catch {
      toast.error("Couldn't copy to clipboard")
    }
  }

  return (
    <Card className="shadow-lg ring-1 ring-border/50">
      <CardHeader>
        <CardTitle>Add a device</CardTitle>
        <CardDescription>
          Link a new machine to your account in three steps.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <ol className="list-inside list-decimal space-y-1 text-sm text-muted-foreground">
          <li>Generate a pairing code below</li>
          <li>
            On the new machine, run{" "}
            <code className="rounded bg-muted px-1 py-0.5 text-xs">
              failsafe pair --code &lt;CODE&gt;
            </code>
          </li>
          <li>The device will appear in your list once paired</li>
        </ol>
        <Button onClick={handleCreatePairingCode} disabled={pairingLoading}>
          {pairingLoading ? "Generating..." : "Get code"}
        </Button>
        {pairing ? (
          <div className="space-y-3 rounded-lg border bg-muted/30 p-4">
            <p
              className={`break-all font-mono text-2xl font-bold tracking-widest select-all sm:text-3xl ${expired ? "text-muted-foreground line-through" : ""}`}
            >
              {pairing.code}
            </p>
            <p className="text-sm text-muted-foreground">{expiryLabel}</p>
            <Button
              variant="secondary"
              size="sm"
              onClick={handleCopyCode}
              disabled={expired}
            >
              {copied ? <Check /> : <Copy />}
              {copied ? "Copied" : "Copy code"}
            </Button>
          </div>
        ) : null}
      </CardContent>
    </Card>
  )
}
