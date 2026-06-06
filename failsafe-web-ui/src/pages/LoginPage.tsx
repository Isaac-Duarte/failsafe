import { useState } from "react"
import { Link, useLocation, useNavigate, useSearchParams } from "react-router-dom"
import { Loader2 } from "lucide-react"

import { AuthCard } from "@failsafe/ui"
import { PasswordInput } from "@failsafe/ui"
import { Alert, AlertDescription } from "@failsafe/ui"
import { Button } from "@failsafe/ui"
import { Input } from "@failsafe/ui"
import { Label } from "@failsafe/ui"
import { login } from "@/lib/api"
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
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault()
    setError(null)
    setLoading(true)

    try {
      const response = await login({ email, password })
      setTokens(response.token, response.refresh_token)
      navigate(redirectTo, { replace: true })
    } catch (err) {
      setError(err instanceof Error ? err.message : "Couldn't sign in")
    } finally {
      setLoading(false)
    }
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
