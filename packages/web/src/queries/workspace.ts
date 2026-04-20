import { useQuery } from "@tanstack/react-query";
import {
  api,
  type OrganizationMembership,
  type OrganizationSummaryResponse,
  type ViewerResponse,
} from "../api";

const VIEWER_QUERY_KEY = ["viewer"] as const;
const ORGANIZATION_SUMMARY_QUERY_KEY = "organization-summary";
const PROJECT_LIST_QUERY_KEY = "project-list";
const WORKSPACE_STALE_TIME_MS = 30_000;

export function useWorkspaceData(preferredOrganizationSlug: string | null) {
  const viewerQuery = useQuery(viewerQueryOptions());

  const activeOrganization = pickActiveOrganization(
    viewerQuery.data,
    preferredOrganizationSlug,
  );

  const projectsQuery = useQuery({
    enabled: Boolean(activeOrganization?.organization_slug),
    queryFn: () => api.listProjects(activeOrganization!.organization_slug),
    queryKey: projectListQueryKey(activeOrganization?.organization_slug ?? ""),
    refetchInterval: WORKSPACE_STALE_TIME_MS,
    staleTime: WORKSPACE_STALE_TIME_MS,
  });

  const summaryQuery = useQuery<
    OrganizationSummaryResponse,
    Error,
    OrganizationSummaryResponse,
    ReturnType<typeof organizationSummaryQueryKey>
  >({
    enabled: Boolean(activeOrganization?.organization_slug),
    queryFn: () =>
      api.getOrganizationSummary(activeOrganization!.organization_slug),
    queryKey: organizationSummaryQueryKey(
      activeOrganization?.organization_slug ?? "",
    ),
    refetchInterval: 15_000,
    staleTime: 15_000,
  });

  return {
    activeOrganization,
    projects: projectsQuery.data?.projects ?? [],
    projectsQuery,
    summaryQuery,
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

export function projectListQueryKey(organizationSlug: string) {
  return [PROJECT_LIST_QUERY_KEY, organizationSlug] as const;
}

export function projectListQueryRootKey() {
  return [PROJECT_LIST_QUERY_KEY] as const;
}

export function organizationSummaryQueryKey(organizationSlug: string) {
  return [ORGANIZATION_SUMMARY_QUERY_KEY, organizationSlug] as const;
}

export function organizationSummaryQueryRootKey() {
  return [ORGANIZATION_SUMMARY_QUERY_KEY] as const;
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
  organizations: OrganizationMembership[],
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
