interface WorkspaceHeaderProps {
  activeOrganizationName: string | null;
  activeOrganizationSlug: string | null;
  isSigningOut: boolean;
  userEmail: string | null;
  onSignOut(): void;
}

export function WorkspaceHeader({
  activeOrganizationName,
  activeOrganizationSlug,
  isSigningOut,
  userEmail,
  onSignOut,
}: WorkspaceHeaderProps) {
  return (
    <section className="room-header">
      <div>
        <div className="section-label">Workspace</div>
        <h1>{activeOrganizationName || "Set up your organization"}</h1>
        <p className="hero-text">
          Authenticate once, pick the active organization, and keep room creation
          and repo joins in the CLI.
        </p>
        {userEmail && (
          <p className="room-meta">
            <span>{userEmail}</span>
            {activeOrganizationSlug && <span>{activeOrganizationSlug}</span>}
          </p>
        )}
      </div>

      <div className="room-header__actions app-toolbar">
        <button
          className="secondary-button"
          type="button"
          disabled={isSigningOut}
          onClick={onSignOut}
        >
          {isSigningOut ? "Signing out..." : "Sign out"}
        </button>
      </div>
    </section>
  );
}
