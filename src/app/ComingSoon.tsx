/** Placeholder for tabs arriving in later phases. */
export function ComingSoon({ title, text }: { title: string; text: string }) {
  return (
    <div className="flex flex-1 items-center justify-center">
      <div className="max-w-sm rounded-lg border border-border bg-card p-6 text-center">
        <h2 className="text-lg font-semibold">{title}</h2>
        <p className="mt-1 text-sm text-neutral-500">{text}</p>
      </div>
    </div>
  );
}
