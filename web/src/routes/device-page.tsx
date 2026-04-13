import { type FormEvent, useEffect, useMemo, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { authClient } from "../auth-client";

type DeviceStatus = "approved" | "denied" | "pending" | null;

export function DevicePage() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const [inputCode, setInputCode] = useState(searchParams.get("user_code") ?? "");
  const [status, setStatus] = useState<DeviceStatus>(null);
  const [error, setError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<"approve" | "deny" | null>(null);

  const userCode = useMemo(
    () => normalizeUserCode(searchParams.get("user_code")),
    [searchParams],
  );

  useEffect(() => {
    setInputCode(searchParams.get("user_code") ?? "");
  }, [searchParams]);

  useEffect(() => {
    if (!userCode) {
      setStatus(null);
      setError(null);
      return;
    }

    let cancelled = false;

    async function loadStatus() {
      const result = await authClient.device({
        query: { user_code: userCode },
      });

      if (cancelled) {
        return;
      }

      if (result.error) {
        setStatus(null);
        setError(readAuthError(result.error));
        return;
      }

      setError(null);
      setStatus(parseDeviceStatus(result.data.status));
    }

    void loadStatus();

    return () => {
      cancelled = true;
    };
  }, [userCode]);

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const normalized = normalizeUserCode(inputCode);
    if (!normalized) {
      setError("Enter the device code from the CLI.");
      return;
    }

    navigate(`/device?user_code=${encodeURIComponent(normalized)}`);
  }

  async function handleAction(action: "approve" | "deny") {
    if (!userCode) {
      return;
    }

    setPendingAction(action);
    setError(null);

    const result =
      action === "approve"
        ? await authClient.device.approve({ userCode })
        : await authClient.device.deny({ userCode });

    setPendingAction(null);

    if (result.error) {
      setError(readAuthError(result.error));
      return;
    }

    setStatus(action === "approve" ? "approved" : "denied");
  }

  return (
    <main className="landing-page">
      <section className="room-header">
        <div>
          <div className="section-label">Device login</div>
          <h1>Approve CLI access</h1>
          <p className="hero-text">
            Confirm the device code from `supermanager login` to let the CLI act as
            your authenticated session.
          </p>
        </div>
      </section>

      <section className="landing-body">
        <div className="landing-column">
          <div className="section-label">Approval</div>

          <form className="room-form room-form--gate" onSubmit={handleSubmit}>
            <label htmlFor="device-code">Device code</label>
            <input
              id="device-code"
              value={inputCode}
              onChange={(event) => setInputCode(event.target.value)}
              placeholder="ABCD-EFGH"
              autoCapitalize="characters"
              autoCorrect="off"
              spellCheck={false}
            />

            <button type="submit">Load device code</button>
          </form>

          {userCode && (
            <div className="device-card">
              <p className="room-meta">
                <span>{userCode}</span>
                {status && (
                  <span className={`summary-pill summary-pill--${statusToTone(status)}`}>
                    {status}
                  </span>
                )}
              </p>

              {error && <p className="message message--error">{error}</p>}

              {!error && !status && <p className="message">Checking device code...</p>}
              {status === "pending" && (
                <>
                  <p className="message">
                    This device is waiting for approval. Approve it to finish CLI login.
                  </p>
                  <div className="device-actions">
                    <button
                      className="secondary-button"
                      type="button"
                      disabled={pendingAction !== null}
                      onClick={() => void handleAction("approve")}
                    >
                      {pendingAction === "approve" ? "Approving..." : "Approve"}
                    </button>
                    <button
                      className="secondary-button"
                      type="button"
                      disabled={pendingAction !== null}
                      onClick={() => void handleAction("deny")}
                    >
                      {pendingAction === "deny" ? "Denying..." : "Deny"}
                    </button>
                  </div>
                </>
              )}
              {status === "approved" && (
                <p className="message">
                  Approved. The CLI can finish logging in now.
                </p>
              )}
              {status === "denied" && (
                <p className="message">Denied. Re-run `supermanager login` to start over.</p>
              )}
            </div>
          )}
        </div>

        <div className="landing-column landing-column--form">
          <div className="section-label">Next</div>
          <p className="message">
            After approval, go back to the terminal and wait for the login command to
            finish.
          </p>
          <Link className="inline-link" to="/app">
            Back to workspace
          </Link>
        </div>
      </section>
    </main>
  );
}

function normalizeUserCode(value: string | null | undefined) {
  const cleaned = value?.trim().toUpperCase().replace(/[^A-Z0-9-]/g, "") ?? "";
  return cleaned || "";
}

function parseDeviceStatus(value: string): DeviceStatus {
  if (value === "approved" || value === "denied" || value === "pending") {
    return value;
  }
  return null;
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

function readAuthError(error: { message?: string; status: number; statusText: string }) {
  return error.message || error.statusText || `Request failed with ${error.status}`;
}
