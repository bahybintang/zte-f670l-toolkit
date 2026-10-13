import { createRootRoute, createRoute, createRouter, Outlet, useRouterState } from '@tanstack/react-router'
import { Shell } from './app/Shell'
import { PageErrorBoundary } from './components/PageErrorBoundary'
import { Dashboard } from './pages/Dashboard'
import { SchemaPage } from './pages/SchemaPage'
import { History } from './pages/History'
import { RawDb } from './pages/RawDb'
import { Clients } from './pages/Clients'
import { Diagnostics } from './pages/Diagnostics'
import { Account } from './pages/Account'
import { NotFound } from './pages/NotFound'
import { Flash } from './pages/Flash'
import { ShareWifi } from './pages/ShareWifi'

function RootLayout() {
  // Key the boundary on the route so navigating away from a crashed page
  // remounts it clean; the boundary sits inside <main>, so the shell survives.
  const path = useRouterState({ select: (s) => s.location.pathname })
  return (
    <Shell>
      <PageErrorBoundary key={path}>
        <Outlet />
      </PageErrorBoundary>
    </Shell>
  )
}

const root = createRootRoute({
  component: RootLayout,
  notFoundComponent: NotFound,
})

const routes = [
  createRoute({ getParentRoute: () => root, path: '/', component: Dashboard }),
  createRoute({ getParentRoute: () => root, path: '/p/$pageId', component: SchemaPage }),
  createRoute({ getParentRoute: () => root, path: '/history', component: History }),
  createRoute({ getParentRoute: () => root, path: '/db', component: RawDb }),
  createRoute({ getParentRoute: () => root, path: '/clients', component: Clients }),
  createRoute({ getParentRoute: () => root, path: '/tools', component: Diagnostics }),
  createRoute({ getParentRoute: () => root, path: '/account', component: Account }),
  createRoute({ getParentRoute: () => root, path: '/flash', component: Flash }),
  createRoute({ getParentRoute: () => root, path: '/share-wifi', component: ShareWifi }),
]

export const router = createRouter({ routeTree: root.addChildren(routes), defaultPreload: 'intent' })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
