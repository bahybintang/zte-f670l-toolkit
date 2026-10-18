/**
 * Calm, intentional placeholder for a live section the firmware has no data path
 * for — so it reads as "not supported here", not as a broken or empty page.
 */
export function NotAvailable({ note }: { note?: string }) {
  return (
    <div className="rounded-lg border border-dashed border-border bg-muted/30 px-4 py-6 text-center">
      <p className="text-sm text-muted-foreground">
        {note ?? 'This readout isn’t available on this firmware.'}
      </p>
    </div>
  )
}
