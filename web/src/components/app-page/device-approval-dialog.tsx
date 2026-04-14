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
  return (
    <div className="dialog-backdrop">
      <div className="dialog-card">
        <div>
          <div className="section-label">CLI Login</div>
          <h2>Approve access</h2>
          <p className="message">
            The CLI is requesting access to your session.
          </p>
          <p className="room-meta">
            <span>{userCode}</span>
            {status && (
              <span className={`summary-pill summary-pill--${statusToTone(status)}`}>
                {status}
              </span>
            )}
          </p>
        </div>

        {error && <p className="message message--error">{error}</p>}
        {!error && !status && <p className="message">Checking device code...</p>}

        {status === "pending" && (
          <div className="dialog-actions">
            <button
              className="primary-button"
              type="button"
              disabled={pendingAction !== null}
              onClick={onApprove}
            >
              {pendingAction === "approve" ? "Approving..." : "Approve"}
            </button>
            <button
              className="secondary-button"
              type="button"
              disabled={pendingAction !== null}
              onClick={onDeny}
            >
              {pendingAction === "deny" ? "Denying..." : "Deny"}
            </button>
          </div>
        )}

        {status === "approved" && (
          <div className="dialog-actions dialog-actions--single">
            <p className="message">
              Approved. Return to the terminal to finish logging in.
            </p>
            <button className="primary-button" type="button" onClick={onClose}>
              Close
            </button>
          </div>
        )}

        {status === "denied" && (
          <div className="dialog-actions dialog-actions--single">
            <p className="message">
              Denied. Re-run `supermanager login` to start over.
            </p>
            <button className="secondary-button" type="button" onClick={onClose}>
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
