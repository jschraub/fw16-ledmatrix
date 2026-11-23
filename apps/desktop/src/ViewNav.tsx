export function ViewNav<T extends string>({ label, items, current, select }: {
  label: string; items: readonly T[]; current: T; select: (item: T) => void;
}) {
  return <nav className="view-nav" aria-label={label}>
    {items.map(item => <button key={item} type="button"
      aria-current={item === current ? "page" : undefined}
      onClick={() => select(item)}>{item}</button>)}
  </nav>;
}
