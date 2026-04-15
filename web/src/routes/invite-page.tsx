import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";
import { authClient, sanitizeReturnTo } from "../auth-client";
import { roomListQueryRootKey, workspaceQueryKey } from "../queries/workspace";
import { readAuthError, readMessage } from "../utils";

export function InvitePage() {
  const { invitationId = "" } = useParams();
  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const session = authClient.useSession();
  const [acceptError, setAcceptError] = useState<string | null>(null);
  const [isAccepting, setIsAccepting] = useState(false);

  const invitationQuery = useQuery({
    enabled: Boolean(invitationId && session.data),
    queryFn: async () => {
      const result = await authClient.organization.getInvitation({
        query: {
          id: invitationId,
        },
      });

      if (result.error) {
        throw new Error(readAuthError(result.error));
      }

      if (!result.data) {
        throw new Error("Invitation not found.");
      }

      return result.data;
    },
    queryKey: ["invitation", invitationId],
    staleTime: 0,
  });

  const invitation = invitationQuery.data ?? null;
  const viewerEmail = session.data?.user.email?.trim().toLowerCase() ?? "";
  const invitedEmail = invitation?.email.trim().toLowerCase() ?? "";
  const isMatchingEmail = !invitation || !viewerEmail || viewerEmail === invitedEmail;
  const isAccepted = invitation?.status === "accepted";
  const isPending = invitation?.status === "pending";
  const returnTo = sanitizeReturnTo(
    `${location.pathname}${location.search}${location.hash}`,
  );

  async function handleAccept() {
    if (!invitationId) {
      return;
    }

    setIsAccepting(true);
    setAcceptError(null);

    const result = await authClient.organization.acceptInvitation({ invitationId });

    setIsAccepting(false);

    if (result.error) {
      setAcceptError(readAuthError(result.error));
      void invitationQuery.refetch();
      return;
    }

    await queryClient.invalidateQueries({ queryKey: workspaceQueryKey() });
    await queryClient.invalidateQueries({ queryKey: roomListQueryRootKey() });
    navigate("/app", { replace: true });
  }

  if (!invitationId) {
    return (
      <main className="shell shell--centered">
        <section className="status-block">
          <div className="section-label">Invite</div>
          <p className="message message--error">Invitation not found.</p>
          <Link className="inline-link" to="/">
            Back
          </Link>
        </section>
      </main>
    );
  }

  if (session.isPending) {
    return (
      <main className="shell shell--centered">
        <section className="status-block">
          <div className="section-label">Invite</div>
          <h1>Checking your session…</h1>
        </section>
      </main>
    );
  }

  if (!session.data) {
    return (
      <main className="shell shell--centered">
        <section className="dialog-card invite-panel">
          <div>
            <div className="section-label">Invite</div>
            <h2>Sign in to join this organization</h2>
            <p className="message">
              Open the invite after you sign in. The account email needs to match
              the invited email.
            </p>
          </div>
          <div className="dialog-actions dialog-actions--single">
            <Link className="primary-button" to={`/login?returnTo=${encodeURIComponent(returnTo)}`}>
              Continue to login
            </Link>
            <Link className="secondary-button" to="/">
              Back
            </Link>
          </div>
        </section>
      </main>
    );
  }

  if (invitationQuery.isLoading) {
    return (
      <main className="shell shell--centered">
        <section className="status-block">
          <div className="section-label">Invite</div>
          <h1>Loading invitation…</h1>
        </section>
      </main>
    );
  }

  if (invitationQuery.isError || !invitation) {
    return (
      <main className="shell shell--centered">
        <section className="dialog-card invite-panel">
          <div>
            <div className="section-label">Invite</div>
            <h2>Invitation unavailable</h2>
            <p className="message message--error">
              {readMessage(invitationQuery.error)}
            </p>
          </div>
          <div className="dialog-actions dialog-actions--single">
            <Link className="secondary-button" to="/app">
              Back to workspace
            </Link>
          </div>
        </section>
      </main>
    );
  }

  return (
    <main className="shell shell--centered">
      <section className="dialog-card invite-panel">
        <div>
          <div className="section-label">Invite</div>
          <h2>Join {invitation.organizationName}</h2>
          <p className="message">
            Accept the invitation to join the organization and open its workspace.
          </p>
        </div>

        <div className="invite-panel__meta">
          <p className="message">
            <strong>Organization:</strong> {invitation.organizationSlug}
          </p>
          <p className="message">
            <strong>Invited email:</strong> {invitation.email}
          </p>
          <p className="message">
            <strong>Signed in as:</strong> {session.data.user.email}
          </p>
          <p className="message">
            <strong>Invited by:</strong> {invitation.inviterEmail}
          </p>
          <p className="message">
            <strong>Status:</strong> {invitation.status}
          </p>
          <p className="message">
            <strong>Expires:</strong> {formatDate(invitation.expiresAt)}
          </p>
        </div>

        {!isMatchingEmail && (
          <p className="message message--error">
            This invite was created for {invitation.email}. Sign out and use that
            email address to accept it.
          </p>
        )}

        {!isAccepted && !isPending && (
          <p className="message message--error">
            This invite is {invitation.status}. Ask the inviter for a new link.
          </p>
        )}

        {acceptError && <p className="message message--error">{acceptError}</p>}

        <div className="dialog-actions dialog-actions--single">
          {isAccepted ? (
            <Link className="primary-button" to="/app">
              Open workspace
            </Link>
          ) : (
            <button
              className="primary-button"
              type="button"
              disabled={isAccepting || !isMatchingEmail || !isPending}
              onClick={() => void handleAccept()}
            >
              {isAccepting ? "Joining..." : "Accept invitation"}
            </button>
          )}
          <Link className="secondary-button" to="/app">
            Back to workspace
          </Link>
        </div>
      </section>
    </main>
  );
}

function formatDate(value: string | Date) {
  const timestamp =
    value instanceof Date ? value.getTime() : Date.parse(value);
  if (Number.isNaN(timestamp)) {
    return String(value);
  }

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(timestamp);
}
