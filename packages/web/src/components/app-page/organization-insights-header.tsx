import { Link } from "react-router-dom";
import { pillBaseClass, roomMetaClass, sectionLabelClass } from "../../ui";

interface OrganizationInsightsHeaderProps {
  organizationName: string | null;
  organizationSlug: string | null;
}

export function OrganizationInsightsHeader({
  organizationName,
  organizationSlug,
}: OrganizationInsightsHeaderProps) {
  const organizationHref = buildOrganizationHref(organizationSlug);
  const label = organizationName || "your org";

  return (
    <header className="flex flex-col gap-7 border-b border-border pb-9 pt-7">
      <div className="max-w-[44rem]">
        <Link
          className="group inline-flex max-w-full flex-wrap items-center gap-3 text-base font-medium text-ink no-underline transition hover:text-white"
          to={organizationHref}
        >
          <span className="font-mono text-[0.72rem] font-semibold uppercase tracking-[0.12em] text-accent transition-transform duration-150 group-hover:-translate-x-px">
            &lt;
          </span>
          <span>{`Back to ${label}`}</span>
        </Link>
        <div className={`${sectionLabelClass} mt-5`}>Organization</div>
        <h1 className="mt-4 max-w-full text-4xl font-semibold leading-none text-ink sm:text-5xl lg:text-6xl">
          Org insights
        </h1>
        <p className={roomMetaClass}>
          <span>{label}</span>
          {organizationSlug && (
            <span className={`${pillBaseClass} border-border text-ink-dim`}>
              {organizationSlug}
            </span>
          )}
        </p>
      </div>
    </header>
  );
}

function buildOrganizationHref(organizationSlug: string | null) {
  if (!organizationSlug) {
    return "/app";
  }

  return `/app?organization=${encodeURIComponent(organizationSlug)}`;
}
