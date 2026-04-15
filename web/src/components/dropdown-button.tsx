import { type ReactNode, useRef } from "react";
import { cx, secondaryButtonClass } from "../ui";

interface DropdownButtonProps {
  children: ReactNode | ((options: { closeDropdown(): void }) => ReactNode);
  className?: string;
  label: string;
  panelClassName?: string;
}

export function DropdownButton({
  children,
  className,
  label,
  panelClassName,
}: DropdownButtonProps) {
  const detailsRef = useRef<HTMLDetailsElement | null>(null);

  function closeDropdown() {
    const details = detailsRef.current;
    if (!details?.open) {
      return;
    }

    details.open = false;
  }

  const panelContent =
    typeof children === "function" ? children({ closeDropdown }) : children;
  const rootClassName = cx(
    "group relative flex w-full flex-col items-end",
    className,
  );
  const panelClasses = cx(
    "absolute right-0 top-[calc(100%+12px)] z-20 w-full border border-border bg-[linear-gradient(180deg,rgba(17,24,37,0.72),rgba(8,12,19,0.88))] p-[22px] shadow-float backdrop-blur-xl",
    panelClassName,
  );

  return (
    <details className={rootClassName} ref={detailsRef}>
      <summary
        className={cx(
          secondaryButtonClass,
          "relative z-30 cursor-pointer list-none justify-center pr-12 text-center text-base font-medium [&::-webkit-details-marker]:hidden [&::marker]:content-[''] after:absolute after:right-[18px] after:top-1/2 after:-translate-y-1/2 after:text-base after:leading-none after:content-['+'] after:transition-transform group-open:after:rotate-45",
        )}
      >
        {label}
      </summary>
      <div
        aria-hidden="true"
        className="fixed inset-0 z-10"
        onClick={closeDropdown}
      />
      <div className={panelClasses}>{panelContent}</div>
    </details>
  );
}
