import { AuthKitProvider, useAuth } from "@workos-inc/authkit-react";
import { type ReactNode, useMemo } from "react";
import { useNavigate } from "react-router-dom";

const WORKOS_CLIENT_ID = import.meta.env.VITE_WORKOS_CLIENT_ID as string | undefined;
const WORKOS_API_HOSTNAME = import.meta.env.VITE_WORKOS_API_HOSTNAME as
  | string
  | undefined;

export { useAuth };

export function SupermanagerAuthProvider({
  children,
}: {
  children: ReactNode;
}) {
  const navigate = useNavigate();
  const redirectUri = useMemo(() => `${window.location.origin}/login`, []);

  if (!WORKOS_CLIENT_ID) {
    throw new Error("VITE_WORKOS_CLIENT_ID is not configured.");
  }

  return (
    <AuthKitProvider
      apiHostname={WORKOS_API_HOSTNAME || undefined}
      clientId={WORKOS_CLIENT_ID}
      devMode={!WORKOS_API_HOSTNAME}
      onRedirectCallback={({ state }) => {
        const next =
          state && typeof state === "object" && typeof state.next === "string"
            ? state.next
            : "/";
        navigate(next, { replace: true });
      }}
      redirectUri={redirectUri}
    >
      {children}
    </AuthKitProvider>
  );
}
