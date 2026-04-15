import { useQuery } from "@tanstack/react-query";
import { api, type ViewerOrganization, type ViewerResponse } from "../api";

const VIEWER_QUERY_KEY = ["viewer"] as const;
const ROOM_LIST_QUERY_KEY = "room-list";
const WORKSPACE_STALE_TIME_MS = 30_000;

export function useWorkspaceData(preferredOrganizationSlug: string | null) {
  const viewerQuery = useQuery(viewerQueryOptions());

  const activeOrganization = pickActiveOrganization(
    viewerQuery.data,
    preferredOrganizationSlug,
  );

  const roomsQuery = useQuery({
    enabled: Boolean(activeOrganization?.organization_slug),
    queryFn: () => api.listRooms(activeOrganization!.organization_slug),
    queryKey: roomListQueryKey(activeOrganization?.organization_slug ?? ""),
    staleTime: WORKSPACE_STALE_TIME_MS,
  });

  return {
    activeOrganization,
    rooms: roomsQuery.data?.rooms ?? [],
    roomsQuery,
    viewerQuery,
  };
}

export function pickActiveOrganization(
  viewer: ViewerResponse | null | undefined,
  preferredOrganizationSlug?: string | null,
) {
  if (!viewer) {
    return null;
  }

  return (
    findOrganizationBySlug(viewer.organizations, preferredOrganizationSlug) ??
    viewer.organizations.find(
      (organization) =>
        organization.organization_id === viewer.active_organization_id,
    ) ??
    viewer.organizations[0] ??
    null
  );
}

export function roomListQueryKey(organizationSlug: string) {
  return [ROOM_LIST_QUERY_KEY, organizationSlug] as const;
}

export function roomListQueryRootKey() {
  return [ROOM_LIST_QUERY_KEY] as const;
}

export function workspaceQueryKey() {
  return VIEWER_QUERY_KEY;
}

export function viewerQueryOptions() {
  return {
    queryFn: api.getMe,
    queryKey: VIEWER_QUERY_KEY,
    staleTime: WORKSPACE_STALE_TIME_MS,
  };
}

export function findOrganizationBySlug(
  organizations: ViewerOrganization[],
  organizationSlug?: string | null,
) {
  if (!organizationSlug) {
    return null;
  }

  return (
    organizations.find(
      (organization) => organization.organization_slug === organizationSlug,
    ) ?? null
  );
}
