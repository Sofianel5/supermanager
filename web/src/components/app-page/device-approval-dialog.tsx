import {
  dialogCardClass,
  errorMessageClass,
  messageClass,
  pillBaseClass,
  primaryButtonClass,
  roomMetaClass,
  secondaryButtonClass,
  sectionLabelClass,
} from "../../ui";

type DeviceStatus = "approved" | "denied" | "pending" | null;

interface DeviceApprovalDialogProps {
  error: string | null;
  pendingAction: "approve" | "deny" | null;
  status: DeviceStatus;
  userCode: string;
  onApprove(): void;
  onClose(): void;
  onDeny(): void;
}

export function DeviceApprovalDialog({
  error,
  pendingAction,
  status,
  userCode,
  onApprove,
  onClose,
  onDeny,
}: DeviceApprovalDialogProps) {
  const isTransientState = status !== "pending" && status !== "approved" && status !== "denied";
  const toneClass = status ? statusToneClass(status) : "";

  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/55 p-5 backdrop-blur-md">
      <div className={`${dialogCardClass} w-full max-w-[440px]`}>
        <div>
          <div className={sectionLabelClass}>CLI Login</div>
          <h2 className="mt-4 text-4xl font-semibold leading-none text-ink sm:text-[2.8rem]">
            Approve access
          </h2>
          <p className={`${messageClass} mt-3`}>
            The CLI is requesting access to your session.
          </p>
          <p className={roomMetaClass}>
            <span>{userCode}</span>
            {status && (
              <span className={`${pillBaseClass} ${toneClass}`}>
                {status}
              </span>
            )}
          </p>
        </div>

        {error && <p className={errorMessageClass}>{error}</p>}
        {!error && !status && <p className={messageClass}>Checking device code...</p>}

        {isTransientState && (
          <div className="grid gap-3">
            <button className={secondaryButtonClass} type="button" onClick={onClose}>
              Close
            </button>
          </div>
        )}

        {status === "pending" && (
          <div className="grid gap-3 sm:grid-cols-2">
            <button
              className={primaryButtonClass}
              type="button"
              disabled={pendingAction !== null}
              onClick={onApprove}
            >
              {pendingAction === "approve" ? "Approving..." : "Approve"}
            </button>
            <button
              className={secondaryButtonClass}
              type="button"
              disabled={pendingAction !== null}
              onClick={onDeny}
            >
              {pendingAction === "deny" ? "Denying..." : "Deny"}
            </button>
          </div>
        )}

        {status === "approved" && (
          <div className="grid gap-3">
            <p className={messageClass}>
              Approved. Return to the terminal to finish logging in.
            </p>
            <button className={primaryButtonClass} type="button" onClick={onClose}>
              Close
            </button>
          </div>
        )}

        {status === "denied" && (
          <div className="grid gap-3">
            <p className={messageClass}>
              Denied. Re-run `supermanager login` to start over.
            </p>
            <button className={secondaryButtonClass} type="button" onClick={onClose}>
              Close
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function statusToTone(status: NonNullable<DeviceStatus>) {
  if (status === "approved") {
    return "ready";
  }
  if (status === "pending") {
    return "generating";
  }
  return "error";
}

function statusToneClass(status: NonNullable<DeviceStatus>) {
  const tone = statusToTone(status);
  if (tone === "ready") {
    return "border-emerald-400/30 text-success";
  }
  if (tone === "generating") {
    return "border-accent/30 text-accent";
  }
  return "border-red-400/30 text-danger";
}
