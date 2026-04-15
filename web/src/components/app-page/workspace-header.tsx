interface WorkspaceHeaderProps {
  activeOrganizationName: string | null;
  activeOrganizationSlug: string | null;
  isSigningOut: boolean;
  userEmail: string | null;
  onOpenInstallInstructions(): void;
  onSignOut(): void;
}

export function WorkspaceHeader({
  activeOrganizationName,
  activeOrganizationSlug,
  isSigningOut,
  userEmail,
  onOpenInstallInstructions,
  onSignOut,
}: WorkspaceHeaderProps) {
  return (
    <section className="room-header">
      <div>
        <div className="section-label">Rooms</div>
        <h1>{activeOrganizationName || "Workspace"}</h1>
        {userEmail && (
          <p className="room-meta">
            <span>{userEmail}</span>
            {activeOrganizationSlug && <span>{activeOrganizationSlug}</span>}
          </p>
        )}
      </div>

      <details className="account-menu">
        <summary className="secondary-button account-menu__trigger">Menu</summary>
        <div className="account-menu__panel">
          <button
            className="account-menu__item"
            type="button"
            onClick={onOpenInstallInstructions}
          >
            CLI install instructions
          </button>
          <button
            className="account-menu__item"
            type="button"
            disabled={isSigningOut}
            onClick={onSignOut}
          >
            {isSigningOut ? "Signing out..." : "Sign out"}
          </button>
        </div>
      </details>
    </section>
  );
}
