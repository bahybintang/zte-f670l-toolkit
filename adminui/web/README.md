# adminui web app

The browser side of [adminui](../README.md): React 19, TypeScript, Vite, Tailwind, shadcn/ui
on Base UI, TanStack Query and TanStack Router. The production build is embedded into the Rust
server binary, so the router serves it with no separate web server.

## Develop

```bash
npm install
npm run dev                                   # http://localhost:5173
ROUTER=http://192.168.1.1:8088 npm run dev    # point /api at another adminui instance
```

The dev server proxies `/api` to a router running adminui (`ROUTER`, default
`http://192.168.1.1:8088`), so you work against real data. Sign in as you would on the router.

| Script | Does |
|---|---|
| `npm run dev` | Vite dev server with hot reload |
| `npm run build` | Type-check (`tsc -b`) and build into `dist/`, which the server embeds |
| `npm run lint` | `oxlint` |

`adminui/scripts/deploy.sh` runs the build for you before compiling the server.

## Layout

| Path | What lives there |
|---|---|
| `src/router.tsx` | Routes. Most pages come from the schema; a few are hand-written |
| `src/pages/SchemaPage.tsx` | Renders any page described in [`../schema/pages/*.json`](../schema/SCHEMA.md) |
| `src/pages/` | Hand-written pages: Overview (`Dashboard`), Devices (`Clients`), EasyMesh, Share Wi-Fi, History, Tools, config DB, Flash, Account, Login |
| `src/components/` | Shared pieces: field controls, apply bar, restart and commit-confirm banners, `NetworkMap`, `MeshMap` |
| `src/components/ui/` | shadcn/ui primitives |
| `src/lib/api.ts` | API client, which sends the CSRF header on every change |
| `src/lib/queries.ts` | Shared TanStack Query hooks for live data (status, hosts, LAN ports, Wi-Fi clients) |

## Conventions

- Formatting: Prettier with single quotes, no semicolons, print width 120.
- Page content comes from the schema. Add or fix a stock page by editing its JSON, not by
  writing a component, so the server's validation and the form stay in sync.
- The server never returns stored passwords, so password fields are write-only.
