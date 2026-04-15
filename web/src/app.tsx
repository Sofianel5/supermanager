import type { ReactNode } from "react";
import { BrowserRouter, Navigate, Route, Routes, useLocation } from "react-router-dom";
import { authClient, sanitizeReturnTo } from "./auth-client";
import { AppPage } from "./routes/app-page";
import { InstallPage } from "./routes/install-page";
import { InvitePage } from "./routes/invite-page";
import { LandingPage } from "./routes/landing-page";
import { LoginPage } from "./routes/login-page";
import { RoomPage } from "./routes/room-page";

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<LandingPage />} />
        <Route path="/install" element={<InstallPage />} />
        <Route path="/invite/:invitationId" element={<InvitePage />} />
        <Route path="/login" element={<LoginPage />} />
        <Route
          path="/app"
          element={
            <RequireSession>
              <AppPage />
            </RequireSession>
          }
        />
        <Route
          path="/r/:roomId"
          element={
            <RequireSession>
              <RoomPage />
            </RequireSession>
          }
        />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}

function RequireSession({ children }: { children: ReactNode }) {
  const location = useLocation();
  const session = authClient.useSession();

  if (session.isPending) {
    return (
      <main className="shell shell--centered">
        <div className="status-block">
          <span className="eyebrow">supermanager</span>
          <h1>Checking your session…</h1>
        </div>
      </main>
    );
  }

  if (!session.data) {
    const returnTo = sanitizeReturnTo(
      `${location.pathname}${location.search}${location.hash}`,
    );
    return (
      <Navigate
        replace
        to={`/login?returnTo=${encodeURIComponent(returnTo)}`}
      />
    );
  }

  return children;
}
