import { AuthKitProvider, useAuth } from "@workos-inc/authkit-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { getApiBaseUrl } from "./api";

export { useAuth };

type AuthConfig = {
  client_id: string;
  api_hostname?: string | null;
};

export function SupermanagerAuthProvider({
  children,
}: {
  children: ReactNode;
}) {
  const navigate = useNavigate();
  const redirectUri = useMemo(() => `${window.location.origin}/login`, []);
  const [config, setConfig] = useState<AuthConfig | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      try {
        const response = await fetch(`${getApiBaseUrl()}/v1/auth/config`);
        if (!response.ok) {
          throw new Error(await response.text() || "Failed to load auth config.");
        }
        const nextConfig = (await response.json()) as AuthConfig;
        if (!cancelled) {
          setConfig(nextConfig);
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(readMessage(loadError));
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  if (error) {
    return <AuthMessage title="Authentication unavailable" message={error} />;
  }

  if (!config) {
    return <AuthMessage title="Loading sign-in" message="Loading authentication…" />;
  }

  return (
    <AuthKitProvider
      apiHostname={config.api_hostname || undefined}
      clientId={config.client_id}
      devMode={!config.api_hostname}
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

function AuthMessage({ title, message }: { title: string; message: string }) {
  return (
    <main className="auth-page">
      <section className="auth-panel">
        <div className="section-label">supermanager</div>
        <h1>{title}</h1>
        <p className="message">{message}</p>
      </section>
    </main>
  );
}

function readMessage(error: unknown) {
  return error instanceof Error ? error.message : "Authentication failed.";
}
