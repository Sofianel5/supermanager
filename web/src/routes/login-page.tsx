import { useEffect, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { useAuth } from "../auth";

export function LoginPage() {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const [error, setError] = useState<string | null>(null);
  const { isLoading, signIn, user } = useAuth();

  const next = params.get("next") || "/";
  const inviteToken = params.get("invitation_token") || undefined;
  const isInvitationRedemption = Boolean(inviteToken);

  useEffect(() => {
    if (!isLoading && user && !isInvitationRedemption) {
      navigate(next, { replace: true });
    }
  }, [isInvitationRedemption, isLoading, navigate, next, user]);

  async function handleContinue() {
    setError(null);
    try {
      await signIn({
        invitationToken: inviteToken,
        state: { next },
      });
    } catch (signInError) {
      setError(readMessage(signInError));
    }
  }

  return (
    <main className="auth-page">
      <section className="auth-panel">
        <div className="section-label">supermanager</div>
        <h1>{isInvitationRedemption ? "Accept invite" : "Sign in"}</h1>
        <p className="message">
          {isInvitationRedemption
            ? "Continue to accept the invitation and open the room."
            : "Continue to WorkOS to choose Google or GitHub."}
        </p>

        {error && <p className="message message--error">{error}</p>}

        <div className="auth-actions">
          <button
            className="inline-link auth-button"
            disabled={isLoading}
            onClick={() => {
              void handleContinue();
            }}
            type="button"
          >
            {isLoading ? "Checking session…" : "Continue"}
          </button>
        </div>

        <Link className="inline-link auth-link" to="/">
          Back to home
        </Link>
      </section>
    </main>
  );
}

function readMessage(error: unknown) {
  return error instanceof Error ? error.message : "Sign-in failed.";
}
