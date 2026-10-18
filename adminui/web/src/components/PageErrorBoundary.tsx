import { Component, type ReactNode } from 'react'
import { Button } from '@/components/ui/button'

interface Props {
  children: ReactNode
}
interface State {
  error: Error | null
}

/**
 * Contains a render error to the page content. The parent gives it a `key` of
 * the current route, so navigating to another page remounts it clean; "Try
 * again" resets it in place. The sidebar and header live outside it and stay
 * usable when a page throws.
 */
export class PageErrorBoundary extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  componentDidCatch(error: Error) {
    console.error('[adminui] page error:', error)
  }

  render() {
    if (!this.state.error) return this.props.children
    return (
      <div className="mx-auto mt-10 max-w-lg rounded-xl border border-destructive/30 bg-destructive/5 p-6 text-center">
        <h2 className="font-heading text-lg font-semibold">This page hit an error</h2>
        <p className="mt-1 text-sm text-muted-foreground">
          Something on this page failed to render. The rest of the app is fine — you can go to another page or try again.
        </p>
        <p className="mt-3 break-words font-mono text-xs text-muted-foreground/80">{this.state.error.message}</p>
        <Button className="mt-4" variant="outline" onClick={() => this.setState({ error: null })}>
          Try again
        </Button>
      </div>
    )
  }
}
