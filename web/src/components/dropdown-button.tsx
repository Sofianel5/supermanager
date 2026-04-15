import { type ReactNode, useRef } from "react";

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
  const rootClassName = ["room-info-dropdown", className].filter(Boolean).join(" ");
  const panelClasses = ["room-info-dropdown__panel", panelClassName]
    .filter(Boolean)
    .join(" ");

  return (
    <details className={rootClassName} ref={detailsRef}>
      <summary className="room-info-dropdown__trigger">{label}</summary>
      <div
        aria-hidden="true"
        className="details-dropdown__backdrop"
        onClick={closeDropdown}
      />
      <div className={panelClasses}>{panelContent}</div>
    </details>
  );
}
