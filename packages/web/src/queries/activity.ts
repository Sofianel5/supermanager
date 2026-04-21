import { api } from "../api";

export const ACTIVITY_LIMIT = 10;

export function organizationUpdatesQueryKey(
  organizationSlug: string,
  limit: number = ACTIVITY_LIMIT,
) {
  return ["organization", organizationSlug, "updates", limit] as const;
}

export function organizationUpdatesQueryOptions(
  organizationSlug: string,
  limit: number = ACTIVITY_LIMIT,
) {
  return {
    queryFn: () => api.getOrganizationUpdates(organizationSlug, { limit }),
    queryKey: organizationUpdatesQueryKey(organizationSlug, limit),
  };
}

export function memberUpdatesQueryKey(
  organizationSlug: string,
  memberUserId: string,
  limit: number = ACTIVITY_LIMIT,
) {
  return ["organization", organizationSlug, "member", memberUserId, "updates", limit] as const;
}

export function memberUpdatesQueryOptions(
  organizationSlug: string,
  memberUserId: string,
  limit: number = ACTIVITY_LIMIT,
) {
  return {
    queryFn: () => api.getMemberUpdates(organizationSlug, memberUserId, { limit }),
    queryKey: memberUpdatesQueryKey(organizationSlug, memberUserId, limit),
  };
}

export function projectUpdatesQueryKey(
  projectId: string,
  limit: number = ACTIVITY_LIMIT,
) {
  return ["project", projectId, "updates", limit] as const;
}

export function projectUpdatesQueryOptions(
  projectId: string,
  limit: number = ACTIVITY_LIMIT,
) {
  return {
    queryFn: () => api.getProjectUpdates(projectId, { limit }),
    queryKey: projectUpdatesQueryKey(projectId, limit),
  };
}
