import { useQuery, useQueryClient } from '@tanstack/react-query'
import { api, setCsrf } from './api'
import type { Schema, SectionData } from './schema'

export interface Session {
  authenticated: boolean
  user?: string
  csrf?: string
  defaultPassword?: boolean
  version?: string
}

export function useSession() {
  return useQuery({
    queryKey: ['session'],
    queryFn: async () => {
      const s = await api.get<Session>('/session')
      if (s.csrf) setCsrf(s.csrf)
      return s
    },
    staleTime: 60_000,
  })
}

export function useSchema(enabled = true) {
  return useQuery({
    queryKey: ['schema'],
    queryFn: () => api.get<Schema>('/schema'),
    staleTime: Infinity,
    enabled,
  })
}

export function usePageData(id: string, sel?: string) {
  return useQuery({
    queryKey: ['page', id, sel ?? ''],
    queryFn: () =>
      api.get<{ sections: Record<string, SectionData> }>(`/page/${id}${sel !== undefined ? `?sel=${encodeURIComponent(sel)}` : ''}`),
  })
}

export interface SystemInfo {
  uptimeSecs: number
  load: number[]
  memTotalKb: number
  memAvailableKb: number
  kernel: string
  cpuCores: number
  ModelName?: string
  SoftwareVer?: string
  HardwareVer?: string
  SerialNumber?: string
  ManuFacturer?: string
}

export function useSystem() {
  return useQuery({
    queryKey: ['system'],
    queryFn: async () => (await api.get<{ system: SystemInfo }>('/status/system')).system,
    refetchInterval: 10_000,
  })
}

export interface IfaceTraffic { name: string; rxBytes: number; txBytes: number; rxRate: number[]; txRate: number[] }
export interface Traffic { intervalSecs: number; cpu: number[]; ifaces: IfaceTraffic[] }

export function useTraffic() {
  return useQuery({
    queryKey: ['traffic'],
    queryFn: async () => (await api.get<{ traffic: Traffic }>('/status/traffic')).traffic,
    refetchInterval: 2_000,
  })
}

export function useInvalidatePages() {
  const qc = useQueryClient()
  return () => qc.invalidateQueries({ queryKey: ['page'] })
}
