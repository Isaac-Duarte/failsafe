import { Link } from "react-router-dom"
import {
  ArrowLeftRight,
  Bell,
  Camera,
  Clipboard,
  FileInput,
  ExternalLink,
  HardDrive,
  Monitor,
  Music,
  Terminal,
} from "lucide-react"
import type { LucideIcon } from "lucide-react"

import {
  Badge,
  Button,
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
  KNOWN_FEATURES,
} from "@failsafe/ui"
import { isAuthenticated } from "@/lib/auth"

const SHIPPED_FEATURE_ICONS: Record<string, LucideIcon> = {
  clipboard: Clipboard,
  shell: Terminal,
  port_forward: ArrowLeftRight,
  file_send: FileInput,
  screen_share: Monitor,
}

const ROADMAP_FEATURES = [
  {
    label: "File copy & paste",
    description: "Copy and paste files between machines",
    icon: FileInput,
  },
  {
    label: "Notification sync",
    description: "See notifications from one device on another",
    icon: Bell,
  },
  {
    label: "Remote desktop",
    description: "Control and view another machine remotely",
    icon: Monitor,
  },
  {
    label: "Shared drive",
    description: "A virtual drive shared across your devices",
    icon: HardDrive,
  },
  {
    label: "Media controls",
    description: "Shared playback controls across devices",
    icon: Music,
  },
  {
    label: "Camera & mic handoff",
    description: "Hand off camera and microphone between machines",
    icon: Camera,
  },
] as const

export function LandingPage() {
  const authenticated = isAuthenticated()

  return (
    <div className="flex w-full flex-col gap-16 py-4 md:gap-20 md:py-8">
      <section className="flex flex-col items-center gap-6 text-center">
        <img
          src="/failsafe-logo.svg"
          alt="Failsafe"
          className="size-20 md:size-24"
        />
        <div className="flex max-w-2xl flex-col gap-4">
          <h1 className="text-4xl font-semibold tracking-tight md:text-5xl">
            Sync across your devices
          </h1>
          <p className="text-base text-muted-foreground md:text-lg">
            An Apple-like experience for cross-machine sync. Pair your devices,
            pick which features each machine enables, and manage everything
            from the web.
          </p>
        </div>
        <div className="flex flex-wrap items-center justify-center gap-3">
          {authenticated ? (
            <Button asChild size="lg">
              <Link to="/devices">Go to devices</Link>
            </Button>
          ) : (
            <>
              <Button asChild size="lg">
                <Link to="/register">Get started</Link>
              </Button>
              <Button asChild variant="outline" size="lg">
                <Link to="/login">Log in</Link>
              </Button>
            </>
          )}
        </div>
      </section>

      <section className="flex flex-col gap-6">
        <div className="text-center">
          <h2 className="text-2xl font-semibold tracking-tight">
            Available now
          </h2>
          <p className="mt-2 text-sm text-muted-foreground">
            Enable per device and start syncing in minutes.
          </p>
        </div>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {KNOWN_FEATURES.map((feature) => {
            const Icon = SHIPPED_FEATURE_ICONS[feature.id] ?? Clipboard
            return (
              <Card
                key={feature.id}
                className="shadow-lg ring-1 ring-border/50"
              >
                <CardHeader>
                  <div className="mb-2 flex size-10 items-center justify-center rounded-lg bg-primary/10 text-primary">
                    <Icon className="size-5" />
                  </div>
                  <CardTitle className="text-lg">{feature.label}</CardTitle>
                  <CardDescription>{feature.description}</CardDescription>
                </CardHeader>
              </Card>
            )
          })}
        </div>
      </section>

      <section className="flex flex-col gap-6">
        <div className="text-center">
          <h2 className="text-2xl font-semibold tracking-tight">
            Coming soon
          </h2>
          <p className="mt-2 text-sm text-muted-foreground">
            More ways to stay connected across your machines.
          </p>
        </div>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {ROADMAP_FEATURES.map((feature) => {
            const Icon = feature.icon
            return (
              <Card
                key={feature.label}
                className="border-dashed bg-muted/30 shadow-none"
              >
                <CardHeader>
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <div className="flex size-10 items-center justify-center rounded-lg bg-muted text-muted-foreground">
                      <Icon className="size-5" />
                    </div>
                    <Badge variant="secondary">Coming soon</Badge>
                  </div>
                  <CardTitle className="text-lg text-muted-foreground">
                    {feature.label}
                  </CardTitle>
                  <CardDescription>{feature.description}</CardDescription>
                </CardHeader>
              </Card>
            )
          })}
        </div>
      </section>

      <footer className="flex justify-center border-t border-border/50 pt-8">
        <Button asChild variant="ghost" size="sm" className="gap-2">
          <a
            href="https://github.com/Isaac-Duarte/failsafe"
            target="_blank"
            rel="noopener noreferrer"
          >
            <ExternalLink className="size-4" />
            View on GitHub
          </a>
        </Button>
      </footer>
    </div>
  )
}
