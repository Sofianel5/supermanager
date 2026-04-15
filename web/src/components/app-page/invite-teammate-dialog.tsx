import { useEffect, useRef, useState } from "react";
import { authClient } from "../../auth-client";
import { readAuthError, useCopyHandler } from "../../utils";

interface InviteTeammateLink {
  email: string;
  inviteUrl: string;
}

interface InviteTeammateDialogProps {
  organizationId: string;
  organizationName: string;
  onClose(): void;
}

export function InviteTeammateDialog({
  organizationId,
  organizationName,
  onClose,
}: InviteTeammateDialogProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const { copiedValue, copy } = useCopyHandler();
  const [email, setEmail] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [invitation, setInvitation] = useState<InviteTeammateLink | null>(null);
  const [isCreating, setIsCreating] = useState(false);

  useEffect(() => {
    if (!invitation) {
      inputRef.current?.focus();
    }
  }, [invitation]);

  function handleClose() {
    if (isCreating) {
      return;
    }

    onClose();
  }

  async function handleInviteSubmit() {
    const normalizedEmail = email.trim().toLowerCase();
    if (!normalizedEmail) {
      setError("Teammate email is required.");
      return;
    }

    setIsCreating(true);
    setError(null);

    try {
      const result = await authClient.organization.inviteMember({
        email: normalizedEmail,
        organizationId,
        role: "member",
      });

      if (result.error) {
        setError(readAuthError(result.error));
        return;
      }

      if (!result.data) {
        setError("Failed to create invite.");
        return;
      }

      setInvitation({
        email: result.data.email,
        inviteUrl: buildInvitationUrl(result.data.id),
      });
    } finally {
      setIsCreating(false);
    }
  }

  function resetInviteForm() {
    setEmail("");
    setError(null);
    setInvitation(null);
  }

  return (
    <div className="dialog-backdrop">
      <div
        className="dialog-card invite-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="invite-teammate-dialog-title"
      >
        <div>
          <div className="section-label">Invite teammate</div>
          <h2 id="invite-teammate-dialog-title">Add someone to {organizationName}</h2>
          <p className="message create-room-dialog__copy">
            Create an email-bound invite link. The recipient needs to sign in with
            that email, then the link takes them straight into the organization.
          </p>
        </div>

        {invitation ? (
          <div className="invite-dialog__result">
            <p className="message">
              Invite ready for <strong>{invitation.email}</strong>.
            </p>
            <button
              className="copy-sheet"
              type="button"
              onClick={() => void copy("invite-link", invitation.inviteUrl)}
            >
              <span className="copy-label">
                Invite link {copiedValue === "invite-link" ? "copied" : "click to copy"}
              </span>
              <code>{invitation.inviteUrl}</code>
            </button>
            <div className="dialog-actions">
              <button className="secondary-button" type="button" onClick={resetInviteForm}>
                Invite another
              </button>
              <a
                className="primary-button"
                href={buildMailtoLink(organizationName, invitation.email, invitation.inviteUrl)}
              >
                Draft email
              </a>
            </div>
            <button className="secondary-button" type="button" onClick={handleClose}>
              Done
            </button>
          </div>
        ) : (
          <form
            className="create-room-dialog__form"
            onSubmit={(event) => {
              event.preventDefault();
              void handleInviteSubmit();
            }}
          >
            <label className="create-room-dialog__label" htmlFor="invite-email">
              Teammate email
            </label>
            <input
              ref={inputRef}
              id="invite-email"
              name="invite-email"
              type="email"
              autoComplete="email"
              spellCheck={false}
              value={email}
              onChange={(event) => setEmail(event.target.value)}
            />

            {error && <p className="message message--error">{error}</p>}

            <div className="dialog-actions">
              <button className="secondary-button" type="button" onClick={handleClose}>
                Cancel
              </button>
              <button className="primary-button" type="submit" disabled={isCreating}>
                {isCreating ? "Creating..." : "Create invite"}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}

function buildMailtoLink(organizationName: string, email: string, inviteUrl: string) {
  const subject = `Join ${organizationName} in supermanager`;
  const body = [
    `You have been invited to join ${organizationName} in supermanager.`,
    "",
    "Open this link and sign in with this email address to join the organization:",
    inviteUrl,
    "",
    `Invited email: ${email}`,
  ].join("\n");

  return `mailto:${encodeURIComponent(email)}?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`;
}

function buildInvitationUrl(invitationId: string) {
  const params = new URLSearchParams({
    invite: invitationId,
  });
  return new URL(`/app?${params.toString()}`, window.location.origin).toString();
}
