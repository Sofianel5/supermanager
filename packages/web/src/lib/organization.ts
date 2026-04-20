export function buildOrganizationHref(organizationSlug: string | null) {
  if (!organizationSlug) {
    return "/app";
  }

  return `/app?organization=${encodeURIComponent(organizationSlug)}`;
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
