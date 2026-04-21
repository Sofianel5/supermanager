export function buildOrganizationHref(organizationSlug: string | null) {
  return buildOrganizationSectionHref("/app", organizationSlug);
}

export function buildOrganizationInsightsHref(organizationSlug: string | null) {
  return buildOrganizationSectionHref("/app/insights", organizationSlug);
}

export function buildOrganizationMemoriesHref(organizationSlug: string | null) {
  return buildOrganizationSectionHref("/app/memories", organizationSlug);
}

export function buildOrganizationSkillsHref(organizationSlug: string | null) {
  return buildOrganizationSectionHref("/app/skills", organizationSlug);
}

export function formatOrganizationLabel(
  organizationName: string | null,
  organizationSlug: string | null,
  fallback: string = "workspace",
) {
  if (organizationName?.trim()) {
    return organizationName.trim();
  }

  if (!organizationSlug) {
    return fallback;
  }

  return organizationSlug
    .split("-")
    .filter(Boolean)
    .map((segment) => segment[0]!.toUpperCase() + segment.slice(1))
    .join(" ");
}

function buildOrganizationSectionHref(path: string, organizationSlug: string | null) {
  if (!organizationSlug) {
    return path;
  }

  return `${path}?organization=${encodeURIComponent(organizationSlug)}`;
}
