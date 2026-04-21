import { cx } from "../ui";

interface MemberAvatarProps {
  className?: string;
  name: string;
  size?: "sm" | "md" | "lg";
}

export function MemberAvatar({ className, name, size = "md" }: MemberAvatarProps) {
  const initials = readInitials(name);
  const sizing =
    size === "lg"
      ? "h-[72px] w-[72px] text-[22px]"
      : size === "sm"
        ? "h-7 w-7 text-[11px]"
        : "h-10 w-10 text-[14px]";

  return (
    <span
      aria-hidden="true"
      className={cx(
        "inline-flex shrink-0 items-center justify-center rounded-full border border-accent/30 bg-[rgba(245,158,11,0.12)] font-mono font-semibold uppercase tracking-[0.04em] text-accent",
        sizing,
        className,
      )}
    >
      {initials}
    </span>
  );
}

function readInitials(name: string) {
  const trimmed = name.trim();
  if (!trimmed) {
    return "?";
  }

  const segments = trimmed
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2);

  if (segments.length === 0) {
    return trimmed.slice(0, 2).toUpperCase();
  }

  if (segments.length === 1) {
    return segments[0]!.slice(0, 2).toUpperCase();
  }

  return `${segments[0]![0]!}${segments[1]![0]!}`.toUpperCase();
}
