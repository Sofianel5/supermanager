import { useMemo, useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { authClient, sanitizeReturnTo, toAbsoluteCallbackUrl } from "../auth-client";
import { readAuthError } from "../utils";

type SocialProvider = "github" | "google";

export function LandingPage() {
  const location = useLocation();
  const session = authClient.useSession();
  const [copiedValue, setCopiedValue] = useState<string | null>(null);
  const [pendingProvider, setPendingProvider] = useState<SocialProvider | null>(null);
  const [authError, setAuthError] = useState<string | null>(null);

  const returnTo = useMemo(
    () =>
      sanitizeReturnTo(new URLSearchParams(location.search).get("returnTo")),
    [location.search],
  );

  async function copy(label: string, value: string) {
    await navigator.clipboard.writeText(value);
    setCopiedValue(label);
    window.setTimeout(() => {
      setCopiedValue((current) => (current === label ? null : current));
    }, 1800);
  }

  async function signIn(provider: SocialProvider) {
    setPendingProvider(provider);
    setAuthError(null);

    const result = await authClient.signIn.social({
      callbackURL: toAbsoluteCallbackUrl(returnTo),
      errorCallbackURL: toAbsoluteCallbackUrl(
        `/?returnTo=${encodeURIComponent(returnTo)}`,
      ),
      provider,
    });

    if (result.error) {
      setPendingProvider(null);
      setAuthError(readAuthError(result.error));
    }
  }

  return (
    <main className="landing-page">
      <section className="landing-hero">
        <div className="hero-copy">
          <div className="eyebrow">supermanager</div>
          <h1>Real-time visibility into your team's AI productivity.</h1>
          <p className="hero-text">
            Sign in with Google or GitHub, create the organization once, then keep
            room creation and repo joins in the CLI.
          </p>
          {session.data && (
            <div className="landing-actions">
              <Link className="inline-link" to={returnTo}>
                Continue to workspace
              </Link>
            </div>
          )}
        </div>
      </section>

      <section className="landing-body">
        <div className="landing-column">
          <div className="section-label">How it works</div>
          <ol className="workflow-list">
            <li>Sign in with Google or GitHub.</li>
            <li>Create your organization and first room.</li>
            <li>
              Run `supermanager login`, then create or join rooms inside each repo.
            </li>
            <li>Claude Code and Codex hook turns flow into the private room dashboard.</li>
          </ol>
        </div>

        <div className="landing-column landing-column--form">
          <div className="section-label">
            {session.data ? "Workspace" : "Sign in"}
          </div>
          {session.isPending ? (
            <p className="message">Checking session...</p>
          ) : session.data ? (
            <>
              <p className="message">
                Signed in as {session.data.user.email}. Continue into the workspace to
                pick an organization and manage rooms.
              </p>
              <div className="auth-actions">
                <Link className="inline-link" to={returnTo}>
                  Continue
                </Link>
              </div>
            </>
          ) : (
            <>
              <p className="message">
                Rooms are private to your organization. Anonymous room access is gone.
              </p>
              <div className="auth-actions">
                <button
                  className="secondary-button auth-button"
                  type="button"
                  disabled={pendingProvider !== null}
                  onClick={() => void signIn("google")}
                >
                  {pendingProvider === "google"
                    ? "Redirecting..."
                    : "Continue with Google"}
                </button>
                <button
                  className="secondary-button auth-button"
                  type="button"
                  disabled={pendingProvider !== null}
                  onClick={() => void signIn("github")}
                >
                  {pendingProvider === "github"
                    ? "Redirecting..."
                    : "Continue with GitHub"}
                </button>
              </div>
              {authError && <p className="message message--error">{authError}</p>}
            </>
          )}

          <div className="section-label auth-section-label">Install</div>
          <button
            className="copy-sheet"
            type="button"
            onClick={() => copy("install", "curl -fsSL https://supermanager.dev/install.sh | sh")}
          >
            <span className="copy-label">
              {copiedValue === "install" ? "copied" : "click to copy"}
            </span>
            <code>curl -fsSL https://supermanager.dev/install.sh | sh</code>
          </button>
        </div>
      </section>
    </main>
  );
}

