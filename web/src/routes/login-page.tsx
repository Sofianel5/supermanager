import { useState } from "react";
import { Link, Navigate, useLocation } from "react-router-dom";
import { authClient, sanitizeReturnTo, toAbsoluteCallbackUrl } from "../auth-client";
import { readAuthError } from "../utils";

type SocialProvider = "github" | "google";

export function LoginPage() {
  const location = useLocation();
  const session = authClient.useSession();
  const [error, setError] = useState<string | null>(null);

  const searchParams = new URLSearchParams(location.search);
  const userCode = normalizeUserCode(searchParams.get("user_code"));
  const sanitizedReturnTo = sanitizeReturnTo(searchParams.get("returnTo"));
  const returnTo = sanitizedReturnTo === "/login" ? "/app" : sanitizedReturnTo;
  const loginPath = buildLoginPath(returnTo, userCode);
  const callbackPath = userCode
    ? `/app?${new URLSearchParams({ user_code: userCode }).toString()}`
    : returnTo;

  async function signIn(provider: SocialProvider) {
    setError(null);

    const result = await authClient.signIn.social({
      callbackURL: toAbsoluteCallbackUrl(callbackPath),
      errorCallbackURL: toAbsoluteCallbackUrl(loginPath),
      provider,
    });

    if (result.error) {
      setError(readAuthError(result.error));
    }
  }

  if (session.isPending) {
    return (
      <main className="shell shell--centered">
        <div className="status-block">
          <span className="eyebrow">supermanager</span>
          <h1>Checking your session…</h1>
        </div>
      </main>
    );
  }

  if (session.data) {
    return <Navigate replace to={callbackPath} />;
  }

  return (
    <main className="shell shell--centered">
      <section className="status-block login-panel">
        <div>
          <div className="eyebrow">supermanager</div>
          <h1>Sign in</h1>
          <p className="message">
            Continue with Google or GitHub to manage rooms and approve CLI logins.
          </p>
        </div>

        <div className="auth-actions">
          <button
            className="secondary-button auth-button"
            type="button"
            onClick={() => void signIn("google")}
          >
            Continue with Google
          </button>
          <button
            className="secondary-button auth-button"
            type="button"
            onClick={() => void signIn("github")}
          >
            Continue with GitHub
          </button>
        </div>
        {error && <p className="message message--error">{error}</p>}
        <Link className="inline-link" to="/">
          Back
        </Link>
      </section>
    </main>
  );
}

function normalizeUserCode(value: string | null | undefined) {
  const cleaned = value?.trim().toUpperCase().replace(/[^A-Z0-9-]/g, "") ?? "";
  return cleaned || "";
}

function buildLoginPath(returnTo: string, userCode: string) {
  const params = new URLSearchParams();
  if (userCode) {
    params.set("user_code", userCode);
  }
  if (returnTo !== "/app") {
    params.set("returnTo", returnTo);
  }
  const query = params.toString();
  return query ? `/login?${query}` : "/login";
}
