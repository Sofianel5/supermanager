import { type InfiniteData, useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { api, type FeedResponse } from "../api";

export const FEED_LIMIT = 10;

const ROOM_STALE_TIME_MS = 30_000;

export function useRoomData(roomId: string) {
  const roomQuery = useQuery({
    enabled: Boolean(roomId),
    ...roomMetadataQueryOptions(roomId),
    staleTime: ROOM_STALE_TIME_MS,
  });

  const summaryQuery = useQuery({
    enabled: Boolean(roomId),
    ...roomSummaryQueryOptions(roomId),
    staleTime: ROOM_STALE_TIME_MS,
  });

  const feedQuery = useInfiniteQuery<
    FeedResponse,
    Error,
    InfiniteData<FeedResponse, number | undefined>,
    ReturnType<typeof roomFeedQueryKey>,
    number | undefined
  >({
    enabled: Boolean(roomId),
    getNextPageParam(lastPage: FeedResponse) {
      if (lastPage.events.length < FEED_LIMIT) {
        return undefined;
      }

      return lastPage.events[lastPage.events.length - 1]?.seq;
    },
    initialPageParam: undefined as number | undefined,
    queryFn: ({ pageParam }) =>
      api.getFeed(roomId, {
        before: pageParam,
        limit: FEED_LIMIT,
      }),
    queryKey: roomFeedQueryKey(roomId),
    staleTime: ROOM_STALE_TIME_MS,
  });

  return {
    feedQuery,
    roomQuery,
    summaryQuery,
  };
}

export function roomMetadataQueryKey(roomId: string) {
  return ["room", roomId, "metadata"] as const;
}

export function roomSummaryQueryKey(roomId: string) {
  return ["room", roomId, "summary"] as const;
}

export function roomFeedQueryKey(roomId: string) {
  return ["room", roomId, "feed"] as const;
}

export function roomSummaryQueryOptions(roomId: string) {
  return {
    queryFn: () => api.getSummary(roomId),
    queryKey: roomSummaryQueryKey(roomId),
  };
}

function roomMetadataQueryOptions(roomId: string) {
  return {
    queryFn: () => api.getRoom(roomId),
    queryKey: roomMetadataQueryKey(roomId),
  };
}
