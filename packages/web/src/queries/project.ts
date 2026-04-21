import { type InfiniteData, useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { api, type FeedResponse } from "../api";
import { ACTIVITY_LIMIT, projectUpdatesQueryOptions } from "./activity";

export const FEED_LIMIT = 10;

const PROJECT_STALE_TIME_MS = 30_000;

export function useProjectData(projectId: string) {
  const projectQuery = useQuery({
    enabled: Boolean(projectId),
    ...projectMetadataQueryOptions(projectId),
    staleTime: PROJECT_STALE_TIME_MS,
  });

  const summaryQuery = useQuery({
    enabled: Boolean(projectId),
    ...projectSummaryQueryOptions(projectId),
    staleTime: PROJECT_STALE_TIME_MS,
  });

  const updatesQuery = useQuery({
    enabled: Boolean(projectId),
    ...projectUpdatesQueryOptions(projectId, ACTIVITY_LIMIT),
    refetchInterval: 15_000,
    staleTime: 15_000,
  });

  const feedQuery = useInfiniteQuery<
    FeedResponse,
    Error,
    InfiniteData<FeedResponse, number | undefined>,
    ReturnType<typeof projectFeedQueryKey>,
    number | undefined
  >({
    enabled: Boolean(projectId),
    getNextPageParam(lastPage: FeedResponse) {
      if (lastPage.events.length < FEED_LIMIT) {
        return undefined;
      }

      return lastPage.events[lastPage.events.length - 1]?.seq;
    },
    initialPageParam: undefined as number | undefined,
    queryFn: ({ pageParam }) =>
      api.getFeed(projectId, {
        before: pageParam,
        limit: FEED_LIMIT,
      }),
    queryKey: projectFeedQueryKey(projectId),
    staleTime: PROJECT_STALE_TIME_MS,
  });

  return {
    feedQuery,
    projectQuery,
    summaryQuery,
    updatesQuery,
  };
}

export function projectMetadataQueryKey(projectId: string) {
  return ["project", projectId, "metadata"] as const;
}

export function projectSummaryQueryKey(projectId: string) {
  return ["project", projectId, "summary"] as const;
}

export function projectFeedQueryKey(projectId: string) {
  return ["project", projectId, "feed"] as const;
}

export function projectSummaryQueryOptions(projectId: string) {
  return {
    queryFn: () => api.getSummary(projectId),
    queryKey: projectSummaryQueryKey(projectId),
  };
}

function projectMetadataQueryOptions(projectId: string) {
  return {
    queryFn: () => api.getProject(projectId),
    queryKey: projectMetadataQueryKey(projectId),
  };
}
