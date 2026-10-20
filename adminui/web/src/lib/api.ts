let csrf = ''

export function setCsrf(token: string) {
  csrf = token
}

export class ApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

type Json = Record<string, unknown>

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = {}
  if (body !== undefined) headers['content-type'] = 'application/json'
  if (method !== 'GET') headers['x-csrf-token'] = csrf
  let res: Response
  try {
    res = await fetch(`/api${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: 'same-origin',
    })
  } catch {
    throw new ApiError(0, 'The router is not answering. Check the cable or wait for it to finish restarting.')
  }
  const data = (await res.json().catch(() => ({}))) as Json
  if (!res.ok || data.ok === false) {
    if (res.status === 401) window.dispatchEvent(new Event('adminui:unauthorized'))
    throw new ApiError(res.status, String(data.error ?? `HTTP ${res.status}`))
  }
  return data as T
}

export const api = {
  get: <T>(path: string) => request<T>('GET', path),
  post: <T>(path: string, body: unknown = {}) => request<T>('POST', path, body),
}
