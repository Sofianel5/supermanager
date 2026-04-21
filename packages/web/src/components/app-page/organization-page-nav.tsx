import { Link } from "react-router-dom";
import {
  buildOrganizationHref,
  buildOrganizationInsightsHref,
  buildOrganizationMemoriesHref,
  buildOrganizationSkillsHref,
} from "../../lib/organization";
import { cx, secondaryButtonClass } from "../../ui";

export type OrganizationPageView = "projects" | "insights" | "memories" | "skills";

interface OrganizationPageNavProps {
  activeView: OrganizationPageView;
  organizationSlug: string | null;
}

export function OrganizationPageNav({
  activeView,
  organizationSlug,
}: OrganizationPageNavProps) {
  const items = [
    { id: "projects", label: "Projects", to: buildOrganizationHref(organizationSlug) },
    {
      id: "insights",
      label: "Insights",
      to: buildOrganizationInsightsHref(organizationSlug),
    },
    {
      id: "memories",
      label: "Memories",
      to: buildOrganizationMemoriesHref(organizationSlug),
    },
    { id: "skills", label: "Skills", to: buildOrganizationSkillsHref(organizationSlug) },
  ] satisfies Array<{ id: OrganizationPageView; label: string; to: string }>;

  return (
    <nav className="mt-7 flex flex-wrap gap-3" aria-label="Organization sections">
      {items.map((item) => (
        <Link
          className={cx(
            secondaryButtonClass,
            item.id === activeView && "border-accent/40 bg-white/6 text-accent",
          )}
          key={item.id}
          to={item.to}
        >
          {item.label}
        </Link>
      ))}
    </nav>
  );
}
