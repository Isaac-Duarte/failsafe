import { useState } from "react"
import { Link, useNavigate } from "react-router-dom"
import { Loader2 } from "lucide-react"

import { AuthCard } from "@failsafe/ui"
import { PasswordInput } from "@failsafe/ui"
import { Alert, AlertDescription } from "@failsafe/ui"
import { Button } from "@failsafe/ui"
import { Input } from "@failsafe/ui"
import { Label } from "@failsafe/ui"
import { register } from "@/lib/api"
import { setToken } from "@/lib/auth"

export function RegisterPage() {
  const navigate = useNavigate()
  const [email, setEmail] = useState("")
  const [password, setPassword] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault()
    setError(null)
    setLoading(true)

    try {
      const response = await register({ email, password })
      setToken(response.token)
      navigate("/devices", { replace: true })
    } catch (err) {
      setError(err instanceof Error ? err.message : "Couldn't create account")
    } finally {
      setLoading(false)
    }
  }

  return (
    <AuthCard
      title="Create account"
      description="Register to start pairing your devices."
      footer={
        <p className="mt-4 text-center text-sm text-muted-foreground">
          Already have an account?{" "}
          <Link
            className="text-primary underline-offset-4 hover:underline"
            to="/login"
          >
            Log in
          </Link>
        </p>
      }
    >
      <form className="space-y-4" onSubmit={handleSubmit}>
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
            autoComplete="new-password"
            required
            value={password}
            onChange={setPassword}
            disabled={loading}
          />
        </div>
        <Button className="w-full" type="submit" disabled={loading}>
          {loading ? <Loader2 className="animate-spin" /> : null}
          {loading ? "Creating account..." : "Register"}
        </Button>
      </form>
    </AuthCard>
  )
}
