import { useState } from "react"
import { QRCodeSVG } from "qrcode.react"
import { Check, Copy, Loader2, Shield, ShieldOff } from "lucide-react"
import { toast } from "sonner"

import {
  Alert,
  AlertDescription,
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Input,
  Label,
  PasswordInput,
} from "@failsafe/ui"
import { useAccount } from "@/hooks/useAccount"
import {
  changePassword,
  disableTotp,
  enableTotp,
  setupTotp,
} from "@/lib/api"
import type { TotpSetupResponse } from "@failsafe/ui"

export function AccountPage() {
  const { email, totpEnabled, loading, refresh } = useAccount()

  const [currentPassword, setCurrentPassword] = useState("")
  const [newPassword, setNewPassword] = useState("")
  const [confirmNewPassword, setConfirmNewPassword] = useState("")
  const [passwordLoading, setPasswordLoading] = useState(false)
  const [passwordError, setPasswordError] = useState<string | null>(null)

  const [setup, setSetup] = useState<TotpSetupResponse | null>(null)
  const [verificationCode, setVerificationCode] = useState("")
  const [recoveryCodes, setRecoveryCodes] = useState<string[] | null>(null)
  const [totpLoading, setTotpLoading] = useState(false)
  const [totpError, setTotpError] = useState<string | null>(null)
  const [copiedSecret, setCopiedSecret] = useState(false)
  const [copiedRecovery, setCopiedRecovery] = useState(false)

  const [disablePassword, setDisablePassword] = useState("")
  const [disableCode, setDisableCode] = useState("")
  const [disableLoading, setDisableLoading] = useState(false)

  async function handleChangePassword(event: React.FormEvent) {
    event.preventDefault()
    setPasswordError(null)

    if (newPassword.length < 8) {
      setPasswordError("New password must be at least 8 characters")
      return
    }

    if (newPassword !== confirmNewPassword) {
      setPasswordError("New passwords do not match")
      return
    }

    setPasswordLoading(true)
    try {
      await changePassword({
        current_password: currentPassword,
        new_password: newPassword,
      })
      setCurrentPassword("")
      setNewPassword("")
      setConfirmNewPassword("")
      toast.success("Password updated")
    } catch (err) {
      setPasswordError(
        err instanceof Error ? err.message : "Couldn't change password"
      )
    } finally {
      setPasswordLoading(false)
    }
  }

  async function handleStartTotpSetup() {
    setTotpError(null)
    setTotpLoading(true)
    setRecoveryCodes(null)

    try {
      const response = await setupTotp()
      setSetup(response)
      setVerificationCode("")
    } catch (err) {
      setTotpError(
        err instanceof Error ? err.message : "Couldn't start 2FA setup"
      )
    } finally {
      setTotpLoading(false)
    }
  }

  async function handleEnableTotp(event: React.FormEvent) {
    event.preventDefault()
    setTotpError(null)
    setTotpLoading(true)

    try {
      const response = await enableTotp({ code: verificationCode })
      setRecoveryCodes(response.recovery_codes)
      setSetup(null)
      setVerificationCode("")
      await refresh()
      toast.success("Two-factor authentication enabled")
    } catch (err) {
      setTotpError(
        err instanceof Error ? err.message : "Couldn't enable 2FA"
      )
    } finally {
      setTotpLoading(false)
    }
  }

  async function handleDisableTotp(event: React.FormEvent) {
    event.preventDefault()
    setDisableLoading(true)
    setTotpError(null)

    try {
      await disableTotp({
        password: disablePassword,
        code: disableCode,
      })
      setDisablePassword("")
      setDisableCode("")
      setSetup(null)
      setRecoveryCodes(null)
      await refresh()
      toast.success("Two-factor authentication disabled")
    } catch (err) {
      setTotpError(
        err instanceof Error ? err.message : "Couldn't disable 2FA"
      )
    } finally {
      setDisableLoading(false)
    }
  }

  async function handleCopySecret() {
    if (!setup?.secret) {
      return
    }

    try {
      await navigator.clipboard.writeText(setup.secret)
      setCopiedSecret(true)
      window.setTimeout(() => setCopiedSecret(false), 2000)
    } catch {
      toast.error("Couldn't copy to clipboard")
    }
  }

  async function handleCopyRecoveryCodes() {
    if (!recoveryCodes) {
      return
    }

    try {
      await navigator.clipboard.writeText(recoveryCodes.join("\n"))
      setCopiedRecovery(true)
      toast.success("Recovery codes copied")
      window.setTimeout(() => setCopiedRecovery(false), 2000)
    } catch {
      toast.error("Couldn't copy to clipboard")
    }
  }

  if (loading) {
    return (
      <div className="flex justify-center py-16">
        <Loader2 className="size-6 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="mx-auto w-full max-w-2xl space-y-6">
      <div className="rounded-2xl border border-border/65 bg-background/55 p-5 backdrop-blur">
        <Badge variant={totpEnabled ? "default" : "outline"} className="mb-3">
          {totpEnabled ? <Shield className="size-3" /> : null}
          {totpEnabled ? "Protected" : "Security setup"}
        </Badge>
        <h1 className="text-3xl font-semibold tracking-tight">Account</h1>
        <p className="text-sm text-muted-foreground">
          Manage your profile and security settings.
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Profile</CardTitle>
          <CardDescription>Your account email address.</CardDescription>
        </CardHeader>
        <CardContent>
          <p className="rounded-lg border border-border/60 bg-background/45 px-3 py-2 text-sm font-semibold">
            {email ?? "-"}
          </p>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Password</CardTitle>
          <CardDescription>Change your account password.</CardDescription>
        </CardHeader>
        <CardContent>
          <form className="space-y-4" onSubmit={handleChangePassword}>
            {passwordError ? (
              <Alert variant="destructive">
                <AlertDescription>{passwordError}</AlertDescription>
              </Alert>
            ) : null}
            <div className="space-y-2">
              <Label htmlFor="current-password">Current password</Label>
              <PasswordInput
                id="current-password"
                autoComplete="current-password"
                required
                value={currentPassword}
                onChange={setCurrentPassword}
                disabled={passwordLoading}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="new-password">New password</Label>
              <PasswordInput
                id="new-password"
                autoComplete="new-password"
                required
                value={newPassword}
                onChange={setNewPassword}
                disabled={passwordLoading}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="confirm-new-password">Confirm new password</Label>
              <PasswordInput
                id="confirm-new-password"
                autoComplete="new-password"
                required
                value={confirmNewPassword}
                onChange={setConfirmNewPassword}
                disabled={passwordLoading}
              />
            </div>
            <Button type="submit" disabled={passwordLoading}>
              {passwordLoading ? <Loader2 className="animate-spin" /> : null}
              {passwordLoading ? "Updating..." : "Update password"}
            </Button>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <div className="flex items-center justify-between gap-2">
            <div>
              <CardTitle>Two-factor authentication</CardTitle>
              <CardDescription>
                Add an extra layer of security with an authenticator app.
              </CardDescription>
            </div>
            {totpEnabled ? (
              <Badge variant="default">
                <Shield className="size-3" />
                Enabled
              </Badge>
            ) : (
              <Badge variant="outline">Disabled</Badge>
            )}
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {totpError ? (
            <Alert variant="destructive">
              <AlertDescription>{totpError}</AlertDescription>
            </Alert>
          ) : null}

          {recoveryCodes ? (
            <div className="space-y-3 rounded-lg border border-amber-500/30 bg-amber-500/5 p-4">
              <p className="text-sm font-medium">
                Save these recovery codes somewhere safe. Each code can only be
                used once if you lose access to your authenticator.
              </p>
              <ul className="grid grid-cols-2 gap-2 font-mono text-sm">
                {recoveryCodes.map((code) => (
                  <li key={code} className="rounded bg-muted px-2 py-1">
                    {code}
                  </li>
                ))}
              </ul>
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={handleCopyRecoveryCodes}
              >
                {copiedRecovery ? <Check /> : <Copy />}
                {copiedRecovery ? "Copied" : "Copy codes"}
              </Button>
            </div>
          ) : null}

          {!totpEnabled && !setup ? (
            <Button onClick={handleStartTotpSetup} disabled={totpLoading}>
              {totpLoading ? <Loader2 className="animate-spin" /> : <Shield />}
              {totpLoading ? "Starting..." : "Enable 2FA"}
            </Button>
          ) : null}

          {!totpEnabled && setup ? (
            <div className="space-y-4">
              <p className="text-sm text-muted-foreground">
                Scan this QR code with your authenticator app, then enter the
                6-digit code to confirm.
              </p>
              <div className="flex justify-center rounded-lg border bg-white p-4">
                <QRCodeSVG value={setup.otpauth_uri} size={192} />
              </div>
              <div className="space-y-2">
                <Label>Manual entry key</Label>
                <div className="flex items-center gap-2">
                  <code className="flex-1 break-all rounded bg-muted px-2 py-1 text-xs">
                    {setup.secret}
                  </code>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={handleCopySecret}
                  >
                    {copiedSecret ? <Check /> : <Copy />}
                  </Button>
                </div>
              </div>
              <form className="space-y-4" onSubmit={handleEnableTotp}>
                <div className="space-y-2">
                  <Label htmlFor="totp-code">Verification code</Label>
                  <Input
                    id="totp-code"
                    inputMode="numeric"
                    autoComplete="one-time-code"
                    pattern="[0-9]{6}"
                    maxLength={6}
                    required
                    value={verificationCode}
                    onChange={(event) =>
                      setVerificationCode(event.target.value)
                    }
                    disabled={totpLoading}
                    placeholder="000000"
                  />
                </div>
                <div className="flex gap-2">
                  <Button type="submit" disabled={totpLoading}>
                    {totpLoading ? (
                      <Loader2 className="animate-spin" />
                    ) : null}
                    {totpLoading ? "Verifying..." : "Confirm and enable"}
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    disabled={totpLoading}
                    onClick={() => setSetup(null)}
                  >
                    Cancel
                  </Button>
                </div>
              </form>
            </div>
          ) : null}

          {totpEnabled ? (
            <form className="space-y-4" onSubmit={handleDisableTotp}>
              <p className="text-sm text-muted-foreground">
                To disable 2FA, enter your password and a current authenticator
                code or recovery code.
              </p>
              <div className="space-y-2">
                <Label htmlFor="disable-password">Password</Label>
                <PasswordInput
                  id="disable-password"
                  autoComplete="current-password"
                  required
                  value={disablePassword}
                  onChange={setDisablePassword}
                  disabled={disableLoading}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="disable-code">Authenticator or recovery code</Label>
                <Input
                  id="disable-code"
                  required
                  value={disableCode}
                  onChange={(event) => setDisableCode(event.target.value)}
                  disabled={disableLoading}
                />
              </div>
              <Button
                type="submit"
                variant="destructive"
                disabled={disableLoading}
              >
                {disableLoading ? (
                  <Loader2 className="animate-spin" />
                ) : (
                  <ShieldOff />
                )}
                {disableLoading ? "Disabling..." : "Disable 2FA"}
              </Button>
            </form>
          ) : null}
        </CardContent>
      </Card>
    </div>
  )
}
