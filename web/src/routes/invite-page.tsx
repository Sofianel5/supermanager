import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useAuth } from "../auth";
import { ApiError, api } from "../api";

export function InvitePage() {
  const { token = "" } = useParams();
  const navigate = useNavigate();
  const { getAccessToken, isLoading, user } = useAuth();
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!token) {
      setError("Invite link is missing.");
      return;
    }

    if (isLoading) {
      return;
    }

    if (!user) {
      navigate(
        `/login?next=${encodeURIComponent(`/invite/${token}`)}`,
        { replace: true },
      );
      return;
    }

    let cancelled = false;
    (async () => {
      try {
        const accessToken = await getAccessToken();
        const response = await api.acceptInvite(accessToken, token);
        if (!cancelled) {
          navigate(`/r/${response.room.room_id}`, { replace: true });
        }
      } catch (loadError) {
        if (cancelled) {
          return;
        }
        if (loadError instanceof ApiError && loadError.status === 401) {
          navigate(
            `/login?next=${encodeURIComponent(`/invite/${token}`)}`,
            { replace: true },
          );
          return;
        }
        setError(readMessage(loadError));
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [getAccessToken, isLoading, navigate, token, user]);

  return (
    <main className="auth-page">
      <section className="auth-panel">
        <div className="section-label">Invite</div>
        <h1>Joining room</h1>
        <p className="message">
          {error || "Checking the invite and adding you to the room."}
        </p>
        <Link className="inline-link auth-link" to="/">
          Back to home
        </Link>
      </section>
    </main>
  );
}

function readMessage(error: unknown) {
  return error instanceof Error ? error.message : "Request failed.";
}
