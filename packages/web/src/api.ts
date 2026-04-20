import type {
  CreateProjectResponse,
  FeedResponse,
  ProjectMetadataResponse,
} from "@supermanager/common/api-protocol";
import type {
  OrganizationSummaryResponse,
  ProjectListResponse,
  ProjectSummaryResponse,
  ViewerResponse,
} from "@supermanager/common/http-types";

export type {
  CreateProjectRequest,
  CreateProjectResponse,
  FeedResponse,
  ProjectMetadataResponse,
  StoredHookEvent,
} from "@supermanager/common/api-protocol";
export type {
  OrganizationMembership,
  OrganizationSummaryResponse,
  ProjectListEntry,
  ProjectListResponse,
  ProjectSummaryResponse,
  ViewerResponse,
  ViewerUser,
} from "@supermanager/common/http-types";
export type {
  MemberSnapshot,
  OrganizationSnapshot,
  ProjectBlufSnapshot,
  ProjectSnapshot,
  SummaryStatus,
} from "@supermanager/common/summary-protocol";

const API_BASE_URL = normalizeBaseUrl(
  import.meta.env.VITE_API_BASE_URL || "http://127.0.0.1:8787",
);

function normalizeBaseUrl(url: string) {
  return url.replace(/\/+$/, "");
}

function apiUrl(path: string) {
  return `${API_BASE_URL}${path}`;
}

async function readError(response: Response) {
  const body = await response.text();
  return body || `Request failed with ${response.status}`;
}

async function requestJson<T>(path: string, init: RequestInit = {}) {
  const response = await fetch(apiUrl(path), {
    credentials: "include",
    ...init,
  });
  if (!response.ok) {
    throw new Error(await readError(response));
  }
  return (await response.json()) as T;
}

export function getApiBaseUrl() {
  return API_BASE_URL;
}

export const api = {
  createProject(input: { name: string; organizationSlug?: string | null }) {
    return requestJson<CreateProjectResponse>("/v1/projects", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        name: input.name,
        organization_slug: input.organizationSlug ?? undefined,
      }),
    });
  },
  getFeed(projectId: string, opts: { limit?: number; before?: number } = {}) {
    const params = new URLSearchParams();
    if (opts.limit != null) params.set("limit", String(opts.limit));
    if (opts.before != null) params.set("before", String(opts.before));
    const qs = params.toString();
    const suffix = qs ? `?${qs}` : "";
    return requestJson<FeedResponse>(
      `/v1/projects/${encodeURIComponent(projectId)}/feed${suffix}`,
    );
  },
  getMe() {
    return requestJson<ViewerResponse>("/v1/me");
  },
  getProject(projectId: string) {
    return requestJson<ProjectMetadataResponse>(
      `/v1/projects/${encodeURIComponent(projectId)}`,
    );
  },
  getSummary(projectId: string) {
    return requestJson<ProjectSummaryResponse>(
      `/v1/projects/${encodeURIComponent(projectId)}/summary`,
    );
  },
  getOrganizationSummary(organizationSlug: string) {
    return requestJson<OrganizationSummaryResponse>(
      `/v1/organizations/${encodeURIComponent(organizationSlug)}/summary`,
    );
  },
  listProjects(organizationSlug?: string) {
    const params = new URLSearchParams();
    if (organizationSlug) {
      params.set("organization_slug", organizationSlug);
    }
    const qs = params.toString();
    return requestJson<ProjectListResponse>(`/v1/projects${qs ? `?${qs}` : ""}`);
  },
  openProjectStream(projectId: string) {
    return new EventSource(
      apiUrl(`/v1/projects/${encodeURIComponent(projectId)}/feed/stream`),
      { withCredentials: true },
    );
  },
};
