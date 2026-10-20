import {
  Activity, AppWindow, Cable, Gauge, Globe, HardDrive, History, KeyRound, LayoutDashboard, LifeBuoy, ListTree,
  Network, Phone, Radio, Router, ScanSearch, Shield, ShieldCheck, Signal, SlidersHorizontal, Stethoscope, Timer,
  Users, Wifi, Wrench, type LucideIcon,
} from 'lucide-react'

const BY_NAME: Record<string, LucideIcon> = {
  activity: Activity, app: AppWindow, cable: Cable, gauge: Gauge, globe: Globe, 'hard-drive': HardDrive,
  history: History, key: KeyRound, dashboard: LayoutDashboard, help: LifeBuoy, tree: ListTree, network: Network,
  phone: Phone, radio: Radio, router: Router, scan: ScanSearch, shield: Shield, 'shield-check': ShieldCheck,
  signal: Signal, sliders: SlidersHorizontal, stethoscope: Stethoscope, timer: Timer, users: Users, wifi: Wifi,
  wrench: Wrench,
}

export const GROUP_ICON: Record<string, LucideIcon> = {
  status: Gauge,
  network: Network,
  wlan: Wifi,
  security: Shield,
  application: AppWindow,
  admin: SlidersHorizontal,
  diagnostics: Stethoscope,
}

export function iconFor(name?: string, group?: string): LucideIcon {
  return (name && BY_NAME[name]) || (group && GROUP_ICON[group]) || ListTree
}
