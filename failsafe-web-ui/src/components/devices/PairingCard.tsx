import { useEffect, useState } from "react"
import { Check, Copy, KeyRound, Terminal } from "lucide-react"
import { toast } from "sonner"

import { Badge, Button } from "@failsafe/ui"
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

function getExpiryState(expiresAt: string): {
  expired: boolean
  label: string
} {
  if (isExpired(expiresAt)) {
    return {
      expired: true,
      label: "Expired - generate a new code",
    }
  }

  return {
    expired: false,
    label: `Expires in ${formatExpiry(expiresAt)}`,
  }
}

export function PairingCard() {
  const [pairing, setPairing] = useState<PairingCreateResponse | null>(null)
  const [pairingLoading, setPairingLoading] = useState(false)
  const [copied, setCopied] = useState(false)
  const [expiry, setExpiry] = useState({ expired: false, label: "" })

  useEffect(() => {
    if (!pairing) {
      return
    }

    const updateExpiry = () => setExpiry(getExpiryState(pairing.expires_at))

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
      setExpiry(getExpiryState(response.expires_at))
    } catch (err) {
      toast.error(
        err instanceof Error ? err.message : "Couldn't create pairing code"
      )
    } finally {
      setPairingLoading(false)
    }
  }

  async function handleCopyCode() {
    if (!pairing || expiry.expired) {
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
    <Card>
      <CardHeader>
        <div className="flex items-start justify-between gap-3">
          <div>
            <Badge variant="secondary" className="mb-3">
              <KeyRound />
              Secure handoff
            </Badge>
            <CardTitle className="text-xl">Add a device</CardTitle>
          </div>
          <div className="hidden size-11 items-center justify-center rounded-xl border border-primary/25 bg-primary/10 text-primary sm:flex">
            <Terminal className="size-5" />
          </div>
        </div>
        <CardDescription>
          Generate a short-lived token, run one CLI command, and the machine
          joins your personal fleet.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <ol className="grid gap-2 text-sm text-muted-foreground md:grid-cols-3">
          <li className="rounded-lg border border-border/60 bg-background/45 p-3">
            <span className="mb-1 block font-mono text-xs text-primary">01</span>
            Generate a pairing code below
          </li>
          <li className="rounded-lg border border-border/60 bg-background/45 p-3 md:col-span-2">
            <span className="mb-1 block font-mono text-xs text-primary">02</span>
            On the new machine, run{" "}
            <code className="rounded bg-muted px-1.5 py-0.5 text-xs text-foreground">
              failsafe pair --code &lt;CODE&gt;
            </code>
          </li>
          <li className="rounded-lg border border-border/60 bg-background/45 p-3 md:col-span-3">
            <span className="mb-1 block font-mono text-xs text-primary">03</span>
            The device appears below once the handshake completes
          </li>
        </ol>
        <Button onClick={handleCreatePairingCode} disabled={pairingLoading}>
          {pairingLoading ? "Generating..." : "Get code"}
        </Button>
        {pairing ? (
          <div className="signal-lines space-y-3 rounded-xl border border-primary/25 bg-primary/10 p-4">
            <p
              className={`break-all font-mono text-3xl font-bold tracking-widest select-all sm:text-4xl ${expiry.expired ? "text-muted-foreground line-through" : ""}`}
            >
              {pairing.code}
            </p>
            <p className="text-sm text-muted-foreground">{expiry.label}</p>
            <Button
              variant="secondary"
              size="sm"
              onClick={handleCopyCode}
              disabled={expiry.expired}
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
