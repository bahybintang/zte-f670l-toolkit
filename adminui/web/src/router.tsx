import { createRootRoute, createRoute, createRouter, Outlet } from '@tanstack/react-router'
import { Shell } from './app/Shell'
import { Dashboard } from './pages/Dashboard'
import { SchemaPage } from './pages/SchemaPage'
import { History } from './pages/History'
import { RawDb } from './pages/RawDb'
import { Clients } from './pages/Clients'
import { Diagnostics } from './pages/Diagnostics'
import { Account } from './pages/Account'
import { NotFound } from './pages/NotFound'

const root = createRootRoute({
  component: () => (
    <Shell>
      <Outlet />
    </Shell>
  ),
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
]

export const router = createRouter({ routeTree: root.addChildren(routes), defaultPreload: 'intent' })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
