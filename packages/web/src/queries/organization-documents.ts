import { useQuery } from "@tanstack/react-query";
import {
  api,
  type OrganizationWorkflowDocumentsResponse,
} from "../api";

const ORGANIZATION_DOCUMENTS_QUERY_KEY = "organization-documents";
const DOCUMENTS_STALE_TIME_MS = 30_000;

export type OrganizationDocumentsView = "memories" | "skills";

export function useOrganizationDocuments(
  view: OrganizationDocumentsView,
  organizationSlug: string | null,
  enabled: boolean,
) {
  return useQuery<
    OrganizationWorkflowDocumentsResponse,
    Error,
    OrganizationWorkflowDocumentsResponse,
    ReturnType<typeof organizationDocumentsQueryKey>
  >({
    enabled: enabled && Boolean(organizationSlug),
    queryFn: () =>
      view === "memories"
        ? api.getOrganizationMemories(organizationSlug!)
        : api.getOrganizationSkills(organizationSlug!),
    queryKey: organizationDocumentsQueryKey(view, organizationSlug ?? ""),
    refetchInterval: DOCUMENTS_STALE_TIME_MS,
    staleTime: DOCUMENTS_STALE_TIME_MS,
  });
}

export function organizationDocumentsQueryKey(
  view: OrganizationDocumentsView,
  organizationSlug: string,
) {
  return [ORGANIZATION_DOCUMENTS_QUERY_KEY, view, organizationSlug] as const;
}
