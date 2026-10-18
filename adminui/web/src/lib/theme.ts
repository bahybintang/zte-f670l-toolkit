export type Theme = 'system' | 'light' | 'dark'

const KEY = 'adminui.theme'

export function getTheme(): Theme {
  try {
    return (localStorage.getItem(KEY) as Theme) || 'system'
  } catch {
    return 'system'
  }
}

function apply(t: Theme) {
  const dark = t === 'dark' || (t === 'system' && matchMedia('(prefers-color-scheme: dark)').matches)
  document.documentElement.classList.toggle('dark', dark)
}

export function setTheme(t: Theme) {
  try {
    localStorage.setItem(KEY, t)
  } catch { /* private mode: theme just won't persist */ }
  apply(t)
}

export function initTheme() {
  apply(getTheme())
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => apply(getTheme()))
}
