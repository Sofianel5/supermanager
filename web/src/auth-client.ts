import { createAuthClient } from "better-auth/react";
import {
  deviceAuthorizationClient,
  organizationClient,
} from "better-auth/client/plugins";
import { getApiBaseUrl } from "./api";

export const authClient = createAuthClient({
  basePath: "/api/auth",
  baseURL: getApiBaseUrl(),
  plugins: [organizationClient(), deviceAuthorizationClient()],
});

export function sanitizeReturnTo(value: string | null | undefined) {
  if (!value || !value.startsWith("/")) {
    return "/app";
  }
  return value;
}

export function toAbsoluteCallbackUrl(path: string) {
  return new URL(path, window.location.origin).toString();
}
