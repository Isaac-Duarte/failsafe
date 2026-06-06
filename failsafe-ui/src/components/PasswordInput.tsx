import { useState } from "react"
import { Eye, EyeOff } from "lucide-react"

import { Button } from "./ui/button"
import { Input } from "./ui/input"

interface PasswordInputProps {
  id: string
  autoComplete: string
  value: string
  onChange: (value: string) => void
  disabled?: boolean
  required?: boolean
}

export function PasswordInput({
  id,
  autoComplete,
  value,
  onChange,
  disabled,
  required,
}: PasswordInputProps) {
  const [visible, setVisible] = useState(false)

  return (
    <div className="relative">
      <Input
        id={id}
        type={visible ? "text" : "password"}
        autoComplete={autoComplete}
        required={required}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        disabled={disabled}
        className="pr-10"
      />
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        className="absolute top-1/2 right-1 z-10 -translate-y-1/2 text-muted-foreground"
        onClick={() => setVisible((current) => !current)}
        disabled={disabled}
        aria-label={visible ? "Hide password" : "Show password"}
        aria-pressed={visible}
      >
        {visible ? <EyeOff /> : <Eye />}
      </Button>
    </div>
  )
}
