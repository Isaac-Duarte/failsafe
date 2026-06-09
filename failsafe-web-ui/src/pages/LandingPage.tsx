import { Link } from "react-router-dom"
import {
  ArrowLeftRight,
  Bell,
  Camera,
  Clipboard,
  ExternalLink,
  HardDrive,
  Monitor,
  Music,
  Send,
  Terminal,
} from "lucide-react"
import type { LucideIcon } from "lucide-react"

import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  type FeatureInfo,
} from "@failsafe/ui"
import { useFeatures } from "@/hooks/useFeatures"
import { isAuthenticated } from "@/lib/auth"

const SHIPPED_FEATURE_ICONS: Record<string, LucideIcon> = {
  clipboard: Clipboard,
  shell: Terminal,
  port_forward: ArrowLeftRight,
  file_send: Send,
}

const DEFAULT_SHIPPED_FEATURES: FeatureInfo[] = [
  {
    id: "clipboard",
    label: "Clipboard sync",
    description: "Copy text on one trusted machine and paste it on another.",
  },
  {
    id: "shell",
    label: "Remote shell",
    description: "Open a controlled shell session across paired devices.",
  },
  {
    id: "port_forward",
    label: "Port Forward",
    description: "Accept forwarded TCP connections from other devices.",
  },
  {
    id: "file_send",
    label: "File Send",
    description: "Receive explicit file transfers from other devices.",
  },
]

const ROADMAP_FEATURES = [
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
  const { features } = useFeatures()
  const shippedFeatures =
    features.length > 0 ? features : DEFAULT_SHIPPED_FEATURES

  return (
    <div className="flex w-full flex-col gap-12 py-2 md:gap-16 md:py-6">
      <section className="grid min-h-[calc(100svh-12rem)] items-center gap-8 lg:grid-cols-[1.05fr_0.95fr]">
        <div className="flex max-w-3xl flex-col gap-6">
          <Badge variant="outline" className="w-fit">
            Local fleet, single control plane
          </Badge>
          <div className="space-y-5">
            <h1 className="max-w-3xl text-5xl leading-[0.95] font-semibold tracking-tight text-balance md:text-7xl">
              Failsafe
            </h1>
            <p className="max-w-2xl text-xl leading-8 text-muted-foreground md:text-2xl">
              A personal sync layer for your machines. Pair once, choose the
              capabilities each device should expose, and keep the handoff
              clean.
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            {authenticated ? (
              <Button asChild size="lg">
                <Link to="/devices">Open device fleet</Link>
              </Button>
            ) : (
              <>
                <Button asChild size="lg">
                  <Link to="/register">Start pairing</Link>
                </Button>
                <Button asChild variant="outline" size="lg">
                  <Link to="/login">Log in</Link>
                </Button>
              </>
            )}
          </div>
        </div>

        <div className="signal-hero-panel relative overflow-hidden rounded-2xl border border-border/70 p-5 md:p-7">
          <div className="absolute inset-x-8 top-1/2 h-px bg-primary/30" />
          <div className="absolute top-10 right-12 bottom-10 w-px bg-primary/25" />
          <div className="relative grid min-h-[24rem] grid-cols-2 gap-4">
            <div className="flex flex-col justify-between rounded-xl border border-border/70 bg-background/70 p-4 backdrop-blur">
              <div>
                <p className="text-xs font-semibold tracking-[0.16em] text-muted-foreground uppercase">
                  Host
                </p>
                <p className="mt-2 text-2xl font-semibold tracking-tight">
                  Workstation
                </p>
              </div>
              <div className="space-y-2">
                <Badge>Online</Badge>
                <p className="font-mono text-xs text-muted-foreground">
                  shell / clipboard / ports / files
                </p>
              </div>
            </div>
            <div className="mt-12 flex flex-col justify-between rounded-xl border border-border/70 bg-background/70 p-4 backdrop-blur">
              <div>
                <p className="text-xs font-semibold tracking-[0.16em] text-muted-foreground uppercase">
                  Mobile
                </p>
                <p className="mt-2 text-2xl font-semibold tracking-tight">
                  Laptop
                </p>
              </div>
              <div className="space-y-2">
                <Badge variant="secondary">Paired</Badge>
                <p className="font-mono text-xs text-muted-foreground">
                  trusted handoff
                </p>
              </div>
            </div>
            <div className="col-span-2 flex items-center justify-between rounded-xl border border-primary/30 bg-primary/10 p-4">
              <div className="flex items-center gap-3">
                <img src="/failsafe-logo.svg" alt="" className="size-12" />
                <div>
                  <p className="font-semibold">Signal path active</p>
                  <p className="text-sm text-muted-foreground">
                    Private capabilities routed per device.
                  </p>
                </div>
              </div>
              <span className="hidden font-mono text-xs text-primary sm:block">
                FS-PAIR
              </span>
            </div>
          </div>
        </div>
      </section>

      <section className="flex flex-col gap-5">
        <div>
          <h2 className="text-2xl font-semibold tracking-tight">Capabilities</h2>
          <p className="mt-2 text-sm text-muted-foreground">
            Enable only what each machine should share.
          </p>
        </div>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {shippedFeatures.map((feature) => {
            const Icon = SHIPPED_FEATURE_ICONS[feature.id] ?? Clipboard
            return (
              <Card key={feature.id}>
                <CardHeader>
                  <div className="mb-3 flex size-11 items-center justify-center rounded-xl border border-primary/25 bg-primary/10 text-primary">
                    <Icon className="size-5" />
                  </div>
                  <CardTitle className="text-lg">{feature.label}</CardTitle>
                  <CardDescription>{feature.description}</CardDescription>
                </CardHeader>
                <CardContent>
                  <div className="h-1 rounded-full bg-primary/15">
                    <div className="h-full w-2/3 rounded-full bg-primary" />
                  </div>
                </CardContent>
              </Card>
            )
          })}
        </div>
      </section>

      <section className="flex flex-col gap-6">
        <div>
          <h2 className="text-2xl font-semibold tracking-tight">Queued modules</h2>
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
                className="border-dashed bg-muted/35 shadow-none"
              >
                <CardHeader>
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <div className="flex size-10 items-center justify-center rounded-xl border border-border/70 bg-background/60 text-muted-foreground">
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
