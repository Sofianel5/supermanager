import { useQuery } from "@tanstack/react-query";
import { authClient } from "../auth-client";
import { readAuthError } from "../utils";

export type DeviceStatus = "approved" | "denied" | "pending" | null;

export function useDeviceStatus(userCode: string) {
  return useQuery({
    enabled: Boolean(userCode),
    queryFn: async () => {
      const result = await authClient.device({
        query: { user_code: userCode },
      });

      if (result.error) {
        throw new Error(readAuthError(result.error));
      }

      return parseDeviceStatus(result.data.status);
    },
    queryKey: deviceStatusQueryKey(userCode),
    staleTime: 0,
  });
}

export function deviceStatusQueryKey(userCode: string) {
  return ["device-status", userCode] as const;
}

function parseDeviceStatus(value: string): DeviceStatus {
  if (value === "approved" || value === "denied" || value === "pending") {
    return value;
  }
  return null;
}
