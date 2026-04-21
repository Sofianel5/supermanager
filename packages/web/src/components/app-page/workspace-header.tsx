import { DropdownButton } from "../dropdown-button";
import { projectMetaClass, sectionLabelClass } from "../../ui";

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
    <section className="flex flex-col gap-7 border-b border-border pb-9 pt-7 md:flex-row md:items-end md:justify-between">
      <div>
        <div className={sectionLabelClass}>Your organization</div>
        <h1 className="mt-4 max-w-full text-4xl font-semibold leading-none text-ink sm:text-5xl lg:text-6xl">
          {activeOrganizationName || "Workspace"}
        </h1>
      </div>

      <div className="w-full md:max-w-[19rem]">
        <DropdownButton label="Menu" panelClassName="grid overflow-hidden p-0">
          {({ closeDropdown }) => (
            <>
              <button
                className="border-b border-border bg-transparent px-4 py-3 text-left text-ink transition hover:bg-white/5"
                type="button"
                onClick={() => {
                  closeDropdown();
                  onInviteTeammate();
                }}
              >
                Invite teammate
              </button>
              <button
                className="border-b border-border bg-transparent px-4 py-3 text-left text-ink transition hover:bg-white/5"
                type="button"
                onClick={() => {
                  closeDropdown();
                  onOpenDocs();
                }}
              >
                Docs
              </button>
              <button
                className="bg-transparent px-4 py-3 text-left text-ink transition hover:bg-white/5 disabled:cursor-wait disabled:opacity-70"
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
