import type { ReactNode } from "react";
import { Link } from "react-router-dom";
import { cx } from "../ui";

export interface InnerTabItem<TId extends string = string> {
  id: TId;
  label: string;
  to: string;
  count?: number;
}

interface InnerTabNavProps<TId extends string> {
  activeId: TId;
  ariaLabel: string;
  items: Array<InnerTabItem<TId>>;
  onSelect?: (id: TId) => void;
  trailing?: ReactNode;
}

export function InnerTabNav<TId extends string>({
  activeId,
  ariaLabel,
  items,
  onSelect,
  trailing,
}: InnerTabNavProps<TId>) {
  return (
    <nav
      aria-label={ariaLabel}
      className="mt-7 flex flex-wrap items-end justify-between gap-4 border-b border-border"
    >
      <ul className="m-0 flex flex-wrap gap-7 p-0">
        {items.map((item) => {
          const isActive = item.id === activeId;
          const itemClassName = cx(
            "inline-flex cursor-pointer items-center gap-2 border-b-2 bg-transparent px-0 py-3.5 text-left font-mono text-[11px] font-semibold uppercase tracking-[0.08em] no-underline transition",
            isActive
              ? "border-accent text-accent"
              : "border-transparent text-ink-muted hover:text-ink",
          );
          const countClassName = cx(
            "font-mono text-[10px]",
            isActive ? "text-ink-dim" : "text-ink-muted",
          );

          return (
            <li className="list-none" key={item.id}>
              {onSelect ? (
                <button
                  aria-pressed={isActive}
                  className={itemClassName}
                  onClick={() => onSelect(item.id)}
                  type="button"
                >
                  <span>{item.label}</span>
                  {typeof item.count === "number" ? (
                    <span className={countClassName}>{item.count}</span>
                  ) : null}
                </button>
              ) : (
                <Link
                  aria-current={isActive ? "page" : undefined}
                  className={itemClassName}
                  to={item.to}
                >
                  <span>{item.label}</span>
                  {typeof item.count === "number" ? (
                    <span className={countClassName}>{item.count}</span>
                  ) : null}
                </Link>
              )}
            </li>
          );
        })}
      </ul>
      {trailing ? <div className="pb-2">{trailing}</div> : null}
    </nav>
  );
}
