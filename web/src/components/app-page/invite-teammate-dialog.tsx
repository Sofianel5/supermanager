import { useEffect, useRef } from "react";
import { useCopyHandler } from "../../utils";

export interface InviteTeammateLink {
  email: string;
  inviteUrl: string;
}

interface InviteTeammateDialogProps {
  email: string;
  error?: string | null;
  invitation?: InviteTeammateLink | null;
  isCreating?: boolean;
  organizationName: string;
  onClose(): void;
  onCreate(): void;
  onEmailChange(email: string): void;
  onReset(): void;
}

export function InviteTeammateDialog({
  email,
  error,
  invitation,
  isCreating = false,
  organizationName,
  onClose,
  onCreate,
  onEmailChange,
  onReset,
}: InviteTeammateDialogProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const { copiedValue, copy } = useCopyHandler();

  useEffect(() => {
    if (!invitation) {
      inputRef.current?.focus();
    }
  }, [invitation]);

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
            that email before joining the organization.
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
              <button className="secondary-button" type="button" onClick={onReset}>
                Invite another
              </button>
              <a
                className="primary-button"
                href={buildMailtoLink(organizationName, invitation.email, invitation.inviteUrl)}
              >
                Draft email
              </a>
            </div>
            <button className="secondary-button" type="button" onClick={onClose}>
              Done
            </button>
          </div>
        ) : (
          <form
            className="create-room-dialog__form"
            onSubmit={(event) => {
              event.preventDefault();
              onCreate();
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
              onChange={(event) => onEmailChange(event.target.value)}
            />

            {error && <p className="message message--error">{error}</p>}

            <div className="dialog-actions">
              <button className="secondary-button" type="button" onClick={onClose}>
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
    "Open this link, sign in with this email address, and accept the invitation:",
    inviteUrl,
    "",
    `Invited email: ${email}`,
  ].join("\n");

  return `mailto:${encodeURIComponent(email)}?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`;
}
