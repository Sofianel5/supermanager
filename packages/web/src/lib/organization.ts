export function buildOrganizationHref(organizationSlug: string | null) {
  return buildOrganizationSectionHref("/app", organizationSlug);
}

export function buildOrganizationMembersHref(organizationSlug: string | null) {
  return buildOrganizationSectionHref("/app/members", organizationSlug);
}

export function buildOrganizationActivityHref(organizationSlug: string | null) {
  return buildOrganizationSectionHref("/app/activity", organizationSlug);
}

export function buildOrganizationKnowledgeHref(organizationSlug: string | null) {
  return buildOrganizationSectionHref("/app/knowledge", organizationSlug);
}

export function buildMemberHref(
  memberUserId: string,
  organizationSlug: string | null,
) {
  return buildOrganizationSectionHref(`/m/${memberUserId}`, organizationSlug);
}

export function buildMemberActivityHref(
  memberUserId: string,
  organizationSlug: string | null,
) {
  return buildOrganizationSectionHref(
    `/m/${memberUserId}/activity`,
    organizationSlug,
  );
}

export function buildProjectHref(projectId: string) {
  return `/p/${projectId}`;
}

export function buildProjectMembersHref(projectId: string) {
  return `/p/${projectId}/members`;
}

export function buildProjectKnowledgeHref(projectId: string) {
  return `/p/${projectId}/knowledge`;
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
