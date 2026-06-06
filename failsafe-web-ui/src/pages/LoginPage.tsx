import { useState } from "react"
import { Link, useLocation, useNavigate, useSearchParams } from "react-router-dom"
import { Loader2 } from "lucide-react"

import { AuthCard } from "@failsafe/ui"
import { PasswordInput } from "@failsafe/ui"
import { Alert, AlertDescription } from "@failsafe/ui"
import { Button } from "@failsafe/ui"
import { Input } from "@failsafe/ui"
import { Label } from "@failsafe/ui"
import { login, loginMfa } from "@/lib/api"
import { setTokens } from "@/lib/auth"

export function LoginPage() {
  const navigate = useNavigate()
  const location = useLocation()
  const [searchParams] = useSearchParams()
  const sessionExpired = searchParams.get("session") === "expired"
  const redirectTo =
    (location.state as { from?: { pathname: string } } | null)?.from
      ?.pathname ?? "/devices"

  const [email, setEmail] = useState("")
  const [password, setPassword] = useState("")
  const [mfaToken, setMfaToken] = useState<string | null>(null)
  const [mfaCode, setMfaCode] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault()
    setError(null)
    setLoading(true)

    try {
      const response = await login({ email, password })

      if (response.mfa_required) {
        if (!response.mfa_token) {
          throw new Error("Two-factor authentication is required but no challenge was returned")
        }
        setMfaToken(response.mfa_token)
        return
      }

      if (!response.token || !response.refresh_token) {
        throw new Error("Sign-in succeeded but no session was returned")
      }

      setTokens(response.token, response.refresh_token)
      navigate(redirectTo, { replace: true })
    } catch (err) {
      setError(err instanceof Error ? err.message : "Couldn't sign in")
    } finally {
      setLoading(false)
    }
  }

  async function handleMfaSubmit(event: React.FormEvent) {
    event.preventDefault()
    if (!mfaToken) {
      return
    }

    setError(null)
    setLoading(true)

    try {
      const response = await loginMfa({
        mfa_token: mfaToken,
        code: mfaCode,
      })

      if (!response.token || !response.refresh_token) {
        throw new Error("Verification succeeded but no session was returned")
      }

      setTokens(response.token, response.refresh_token)
      navigate(redirectTo, { replace: true })
    } catch (err) {
      setError(err instanceof Error ? err.message : "Couldn't verify code")
    } finally {
      setLoading(false)
    }
  }

  if (mfaToken) {
    return (
      <AuthCard
        title="Two-factor authentication"
        description="Enter the 6-digit code from your authenticator app."
        footer={
          <p className="mt-4 text-center text-sm text-muted-foreground">
            <button
              type="button"
              className="text-primary underline-offset-4 hover:underline"
              onClick={() => {
                setMfaToken(null)
                setMfaCode("")
                setError(null)
              }}
            >
              Back to sign in
            </button>
          </p>
        }
      >
        <form className="space-y-4" onSubmit={handleMfaSubmit}>
          {error ? (
            <Alert variant="destructive">
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          ) : null}
          <div className="space-y-2">
            <Label htmlFor="mfa-code">Authentication code</Label>
            <Input
              id="mfa-code"
              inputMode="numeric"
              autoComplete="one-time-code"
              pattern="[0-9]{6}"
              maxLength={6}
              required
              autoFocus
              value={mfaCode}
              onChange={(event) => setMfaCode(event.target.value)}
              disabled={loading}
              placeholder="000000"
            />
          </div>
          <Button className="w-full" type="submit" disabled={loading}>
            {loading ? <Loader2 className="animate-spin" /> : null}
            {loading ? "Verifying..." : "Verify"}
          </Button>
        </form>
      </AuthCard>
    )
  }

  return (
    <AuthCard
      title="Log in"
      description="Sign in to manage your devices."
      footer={
        <p className="mt-4 text-center text-sm text-muted-foreground">
          No account?{" "}
          <Link
            className="text-primary underline-offset-4 hover:underline"
            to="/register"
          >
            Register
          </Link>
        </p>
      }
    >
      <form className="space-y-4" onSubmit={handleSubmit}>
        {sessionExpired ? (
          <Alert>
            <AlertDescription>
              Your session expired. Please sign in again.
            </AlertDescription>
          </Alert>
        ) : null}
        {error ? (
          <Alert variant="destructive">
            <AlertDescription>{error}</AlertDescription>
          </Alert>
        ) : null}
        <div className="space-y-2">
          <Label htmlFor="email">Email</Label>
          <Input
            id="email"
            type="email"
            autoComplete="email"
            required
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            disabled={loading}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="password">Password</Label>
          <PasswordInput
            id="password"
            autoComplete="current-password"
            required
            value={password}
            onChange={setPassword}
            disabled={loading}
          />
        </div>
        <Button className="w-full" type="submit" disabled={loading}>
          {loading ? <Loader2 className="animate-spin" /> : null}
          {loading ? "Signing in..." : "Log in"}
        </Button>
      </form>
    </AuthCard>
  )
}
