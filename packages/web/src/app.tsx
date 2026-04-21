import type { ReactNode } from "react";
import { BrowserRouter, Navigate, Route, Routes, useLocation } from "react-router-dom";
import { authClient, sanitizeReturnTo } from "./auth-client";
import { AppPage } from "./routes/app-page";
import { DocsPage } from "./routes/docs-page";
import { LandingPage } from "./routes/landing-page";
import { LoginPage } from "./routes/login-page";
import { MemberPage } from "./routes/member-page";
import { ProjectPage } from "./routes/project-page";
import { centeredShellClass, sectionLabelClass, statusBlockClass } from "./ui";

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<LandingPage />} />
        <Route path="/docs" element={<DocsPage />} />
        <Route path="/login" element={<LoginPage />} />
        <Route
          path="/app"
          element={
            <RequireSession>
              <AppPage view="projects" />
            </RequireSession>
          }
        />
        <Route
          path="/app/activity"
          element={
            <RequireSession>
              <AppPage view="activity" />
            </RequireSession>
          }
        />
        <Route
          path="/app/members"
          element={
            <RequireSession>
              <AppPage view="members" />
            </RequireSession>
          }
        />
        <Route
          path="/app/knowledge"
          element={
            <RequireSession>
              <AppPage view="knowledge" />
            </RequireSession>
          }
        />
        <Route path="/app/insights" element={<RedirectPreserveQuery to="/app" />} />
        <Route
          path="/app/memories"
          element={<RedirectPreserveQuery to="/app/knowledge" />}
        />
        <Route
          path="/app/skills"
          element={<RedirectPreserveQuery to="/app/knowledge" />}
        />
        <Route
          path="/m/:memberId"
          element={
            <RequireSession>
              <MemberPage view="summary" />
            </RequireSession>
          }
        />
        <Route
          path="/m/:memberId/activity"
          element={
            <RequireSession>
              <MemberPage view="activity" />
            </RequireSession>
          }
        />
        <Route
          path="/p/:projectId"
          element={
            <RequireSession>
              <ProjectPage view="activity" />
            </RequireSession>
          }
        />
        <Route
          path="/p/:projectId/members"
          element={
            <RequireSession>
              <ProjectPage view="members" />
            </RequireSession>
          }
        />
        <Route
          path="/p/:projectId/knowledge"
          element={
            <RequireSession>
              <ProjectPage view="knowledge" />
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
      <main className={centeredShellClass}>
        <div className={statusBlockClass}>
          <span className={sectionLabelClass}>supermanager</span>
          <h1 className="mt-4 text-4xl font-semibold leading-none text-ink sm:text-5xl">
            Checking your session…
          </h1>
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

function RedirectPreserveQuery({ to }: { to: string }) {
  const location = useLocation();
  return <Navigate replace to={`${to}${location.search}`} />;
}
