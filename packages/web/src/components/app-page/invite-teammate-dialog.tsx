import { useEffect, useRef, useState } from "react";
import { authClient } from "../../auth-client";
import {
  copyLabelClass,
  copySheetClass,
  dialogCardClass,
  errorMessageClass,
  fieldLabelClass,
  inputClass,
  messageClass,
  primaryButtonClass,
  secondaryButtonClass,
  sectionLabelClass,
} from "../../ui";
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
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/55 p-5 backdrop-blur-md">
      <div
        className={`${dialogCardClass} w-full max-w-[560px]`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="invite-teammate-dialog-title"
      >
        <div>
          <div className={sectionLabelClass}>Invite teammate</div>
          <h2
            className="mt-4 text-4xl font-semibold leading-none text-ink sm:text-[2.8rem]"
            id="invite-teammate-dialog-title"
          >
            Add someone to {organizationName}
          </h2>
          <p className={`${messageClass} mt-3`}>
            Create an email-bound invite link. The recipient needs to sign in with
            that email, then the link takes them straight into the organization.
          </p>
        </div>

        {invitation ? (
          <div className="grid gap-3.5">
            <p className={messageClass}>
              Invite ready for <strong>{invitation.email}</strong>.
            </p>
            <button
              className={copySheetClass}
              type="button"
              onClick={() => void copy("invite-link", invitation.inviteUrl)}
            >
              <span className={copyLabelClass}>
                Invite link {copiedValue === "invite-link" ? "copied" : "click to copy"}
              </span>
              <code className="mt-2.5 block break-words font-mono text-[13px] leading-7 text-[#f4bf63]">
                {invitation.inviteUrl}
              </code>
            </button>
            <div className="grid gap-3 sm:grid-cols-2">
              <button className={secondaryButtonClass} type="button" onClick={resetInviteForm}>
                Invite another
              </button>
              <a
                className={primaryButtonClass}
                href={buildMailtoLink(organizationName, invitation.email, invitation.inviteUrl)}
              >
                Draft email
              </a>
            </div>
            <button className={secondaryButtonClass} type="button" onClick={handleClose}>
              Done
            </button>
          </div>
        ) : (
          <form
            className="grid gap-3.5"
            onSubmit={(event) => {
              event.preventDefault();
              void handleInviteSubmit();
            }}
          >
            <label className={fieldLabelClass} htmlFor="invite-email">
              Teammate email
            </label>
            <input
              className={inputClass}
              ref={inputRef}
              id="invite-email"
              name="invite-email"
              type="email"
              autoComplete="email"
              spellCheck={false}
              value={email}
              onChange={(event) => setEmail(event.target.value)}
            />

            {error && <p className={errorMessageClass}>{error}</p>}

            <div className="grid gap-3 sm:grid-cols-2">
              <button className={secondaryButtonClass} type="button" onClick={handleClose}>
                Cancel
              </button>
              <button className={primaryButtonClass} type="submit" disabled={isCreating}>
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
