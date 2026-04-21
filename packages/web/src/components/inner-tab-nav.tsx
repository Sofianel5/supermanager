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
  trailing?: ReactNode;
}

export function InnerTabNav<TId extends string>({
  activeId,
  ariaLabel,
  items,
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
          return (
            <li className="list-none" key={item.id}>
              <Link
                aria-current={isActive ? "page" : undefined}
                className={cx(
                  "inline-flex items-center gap-2 border-b-2 py-3.5 font-mono text-[11px] font-semibold uppercase tracking-[0.08em] no-underline transition",
                  isActive
                    ? "border-accent text-accent"
                    : "border-transparent text-ink-muted hover:text-ink",
                )}
                to={item.to}
              >
                <span>{item.label}</span>
                {typeof item.count === "number" ? (
                  <span
                    className={cx(
                      "font-mono text-[10px]",
                      isActive ? "text-ink-dim" : "text-ink-muted",
                    )}
                  >
                    {item.count}
                  </span>
                ) : null}
              </Link>
            </li>
          );
        })}
      </ul>
      {trailing ? <div className="pb-2">{trailing}</div> : null}
    </nav>
  );
}
