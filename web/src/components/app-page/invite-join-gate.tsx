import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { authClient } from "../../auth-client";
import { readAuthError } from "../../utils";

interface InviteJoinGateProps {
  onRefreshWorkspace(): Promise<void>;
}

interface AppToast {
  kind: "error" | "success";
  message: string;
}

const joinSuccessToast: AppToast = {
  kind: "success",
  message: "Joined successfully.",
};

export function InviteJoinGate({ onRefreshWorkspace }: InviteJoinGateProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const session = authClient.useSession();
  const handledInviteIdRef = useRef<string | null>(null);
  const [toast, setToast] = useState<AppToast | null>(null);

  const inviteId = new URLSearchParams(location.search).get("invite")?.trim() ?? "";
  const sessionEmail = session.data?.user.email?.trim().toLowerCase() ?? "";

  useEffect(() => {
    if (!inviteId) {
      handledInviteIdRef.current = null;
      return;
    }
    if (!sessionEmail || handledInviteIdRef.current === inviteId) {
      return;
    }

    handledInviteIdRef.current = inviteId;

    let cancelled = false;

    void (async () => {
      const nextToast = await acceptInvite(inviteId, sessionEmail);
      if (cancelled) {
        return;
      }

      if (nextToast.kind === "success") {
        await onRefreshWorkspace();
        if (cancelled) {
          return;
        }
      }

      const params = new URLSearchParams(location.search);
      params.delete("invite");
      const query = params.toString();
      navigate(
        { pathname: location.pathname, search: query ? `?${query}` : "" },
        { replace: true },
      );
      setToast(nextToast);
    })();

    return () => {
      cancelled = true;
    };
  }, [inviteId, sessionEmail, location.pathname, location.search, navigate, onRefreshWorkspace]);

  useEffect(() => {
    if (!toast) {
      return;
    }

    const timeoutId = window.setTimeout(() => {
      setToast(null);
    }, 3200);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [toast]);

  if (!inviteId && !toast) {
    return null;
  }

  return (
    <>
      {inviteId && (
        <div className="dialog-backdrop">
          <section className="status-block">
            <div className="section-label">Organization</div>
            <h1>Joining workspace…</h1>
          </section>
        </div>
      )}

      {toast && (
        <div
          aria-live="polite"
          className={`app-toast app-toast--${toast.kind}`}
          role={toast.kind === "error" ? "alert" : "status"}
        >
          {toast.message}
        </div>
      )}
    </>
  );
}

async function acceptInvite(invitationId: string, sessionEmail: string): Promise<AppToast> {
  const acceptResult = await authClient.organization.acceptInvitation({
    invitationId,
  });

  if (!acceptResult.error) {
    return joinSuccessToast;
  }

  const invitationResult = await authClient.organization.getInvitation({
    query: { id: invitationId },
  });

  const invitedEmail = invitationResult.data?.email.trim().toLowerCase() ?? "";
  if (invitationResult.data?.status === "accepted" && invitedEmail === sessionEmail) {
    return joinSuccessToast;
  }

  return {
    kind: "error",
    message: readAuthError(acceptResult.error),
  };
}
