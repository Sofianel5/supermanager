import { api, type UpdateScope } from "../api";

export const UPDATES_LIMIT = 10;
const UPDATES_STALE_TIME_MS = 30_000;

export function projectUpdatesQueryKey(projectId: string) {
  return ["project", projectId, "updates"] as const;
}

export function organizationUpdatesQueryKey(
  organizationSlug: string,
  scope?: UpdateScope,
) {
  return [
    "organization",
    organizationSlug,
    "updates",
    scope ?? "all",
  ] as const;
}

export function memberUpdatesQueryKey(
  organizationSlug: string,
  memberUserId: string,
) {
  return [
    "organization",
    organizationSlug,
    "members",
    memberUserId,
    "updates",
  ] as const;
}

export function projectUpdatesQueryOptions(projectId: string) {
  return {
    queryFn: () => api.getProjectUpdates(projectId, { limit: UPDATES_LIMIT }),
    queryKey: projectUpdatesQueryKey(projectId),
    staleTime: UPDATES_STALE_TIME_MS,
  };
}

export function organizationUpdatesQueryOptions(
  organizationSlug: string,
  scope?: UpdateScope,
) {
  return {
    queryFn: () =>
      api.getOrganizationUpdates(organizationSlug, {
        limit: UPDATES_LIMIT,
        scope,
      }),
    queryKey: organizationUpdatesQueryKey(organizationSlug, scope),
    staleTime: UPDATES_STALE_TIME_MS,
  };
}

export function memberUpdatesQueryOptions(
  organizationSlug: string,
  memberUserId: string,
) {
  return {
    queryFn: () =>
      api.getMemberUpdates(organizationSlug, memberUserId, {
        limit: UPDATES_LIMIT,
      }),
    queryKey: memberUpdatesQueryKey(organizationSlug, memberUserId),
    staleTime: UPDATES_STALE_TIME_MS,
  };
}
