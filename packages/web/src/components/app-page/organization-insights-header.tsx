import { DropdownButton } from "../dropdown-button";
import { Link } from "react-router-dom";
import { pillBaseClass, roomMetaClass } from "../../ui";

interface OrganizationInsightsHeaderProps {
  organizationName: string | null;
  organizationSlug: string | null;
  isSigningOut: boolean;
  onInviteTeammate(): void;
  onOpenDocs(): void;
  onSignOut(): void;
}

export function OrganizationInsightsHeader({
  organizationName,
  organizationSlug,
  isSigningOut,
  onInviteTeammate,
  onOpenDocs,
  onSignOut,
}: OrganizationInsightsHeaderProps) {
  const organizationHref = buildOrganizationHref(organizationSlug);
  const label = organizationName || "your org";

  return (
    <header className="flex flex-col gap-7 border-b border-border pb-9 pt-7 md:flex-row md:items-start md:justify-between">
      <div className="max-w-[44rem]">
        <Link
          className="group inline-flex max-w-full flex-wrap items-center gap-3 text-base font-medium text-ink no-underline transition hover:text-white"
          to={organizationHref}
        >
          <span className="font-mono text-[0.72rem] font-semibold uppercase tracking-[0.12em] text-accent transition-transform duration-150 group-hover:-translate-x-px">
            &lt;
          </span>
          <span>{`Back to ${label}`}</span>
        </Link>
        <h1 className="mt-5 max-w-full text-4xl font-semibold leading-none text-ink sm:text-5xl lg:text-6xl">
          Org insights
        </h1>
        <p className={roomMetaClass}>
          <span>{label}</span>
          {organizationSlug && (
            <span className={`${pillBaseClass} border-border text-ink-dim`}>
              {organizationSlug}
            </span>
          )}
        </p>
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
    </header>
  );
}

function buildOrganizationHref(organizationSlug: string | null) {
  if (!organizationSlug) {
    return "/app";
  }

  return `/app?organization=${encodeURIComponent(organizationSlug)}`;
}
