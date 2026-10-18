import { Link } from '@tanstack/react-router'

export function NotFound() {
  return (
    <div className="py-16">
      <h1 className="font-heading text-[28px] font-semibold">This page does not exist</h1>
      <p className="mt-2 text-muted-foreground">
        It may have moved. <Link to="/" className="text-link underline underline-offset-2">Go to the overview</Link> or use the
        search in the menu.
      </p>
    </div>
  )
}
