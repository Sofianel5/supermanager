import { Link } from "react-router-dom";
import { cx, secondaryButtonClass } from "../../ui";

interface SecondaryActionLinkProps {
  children: string;
  className?: string;
  to: string;
}

export function SecondaryActionLink({
  children,
  className,
  to,
}: SecondaryActionLinkProps) {
  return (
    <Link className={cx(secondaryButtonClass, className)} to={to}>
      {children}
    </Link>
  );
}
