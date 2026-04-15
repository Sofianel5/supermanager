import { DropdownButton } from "../dropdown-button";

interface WorkspaceHeaderProps {
  activeOrganizationName: string | null;
  activeOrganizationSlug: string | null;
  isSigningOut: boolean;
  userEmail: string | null;
  onInviteTeammate(): void;
  onOpenDocs(): void;
  onSignOut(): void;
}

export function WorkspaceHeader({
  activeOrganizationName,
  activeOrganizationSlug,
  isSigningOut,
  userEmail,
  onInviteTeammate,
  onOpenDocs,
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

      <div className="room-header__actions">
        <DropdownButton label="Menu" panelClassName="room-section account-menu__panel">
          {({ closeDropdown }) => (
            <>
              <button
                className="account-menu__item"
                type="button"
                onClick={() => {
                  closeDropdown();
                  onInviteTeammate();
                }}
              >
                Invite teammate
              </button>
              <button
                className="account-menu__item"
                type="button"
                onClick={() => {
                  closeDropdown();
                  onOpenDocs();
                }}
              >
                Docs
              </button>
              <button
                className="account-menu__item"
                type="button"
                disabled={isSigningOut}
                onClick={() => {
                  closeDropdown();
                  onSignOut();
                }}
              >
                {isSigningOut ? "Signing out..." : "Sign out"}
              </button>
            </>
          )}
        </DropdownButton>
      </div>
    </section>
  );
}
