import { useState } from "react";
import { authClient } from "../../auth-client";
import {
  cx,
  errorMessageClass,
  fieldLabelClass,
  jumboInputClass,
  messageClass,
  pageShellClass,
  primaryButtonClass,
  secondaryButtonClass,
  sectionLabelClass,
  surfaceClass,
} from "../../ui";
import { readAuthError, readMessage } from "../../utils";

interface OrganizationOnboardingProps {
  error: string | null;
  onRefreshWorkspace(): Promise<void>;
  onSignOut(): void;
  userEmail: string | null;
}

export function OrganizationOnboarding({
  error,
  onRefreshWorkspace,
  onSignOut,
  userEmail,
}: OrganizationOnboardingProps) {
  const [organizationName, setOrganizationName] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);

  async function handleCreateOrganization() {
    if (isCreating) {
      return;
    }

    const name = organizationName.trim();
    if (!name) {
      setCreateError("Organization name is required.");
      return;
    }

    const slug = slugifyOrganizationName(name);
    if (!slug) {
      setCreateError("Use letters or numbers in the organization name.");
      return;
    }

    setIsCreating(true);
    setCreateError(null);

    try {
      const result = await authClient.organization.create({
        name,
        slug,
      });

      if (result.error) {
        setCreateError(readAuthError(result.error));
        return;
      }

      await onRefreshWorkspace();
    } catch (error) {
      setCreateError(readMessage(error));
    } finally {
      setIsCreating(false);
    }
  }

  return (
    <main className={cx(pageShellClass, "pt-14")}>
      <div className="flex flex-col gap-7 md:flex-row md:items-start md:justify-between">
        <div className="max-w-[44rem]">
          <div className={sectionLabelClass}>supermanager</div>
          <h1 className="m-0 max-w-[14ch] text-5xl font-semibold leading-none text-ink sm:text-6xl lg:text-[4.6rem]">
            Choose how you want to start.
          </h1>
          <p className="mt-4 max-w-[30rem] text-[1.08rem] leading-8 text-ink-dim">
            Create your organization now, or join one through an invite link.
          </p>
        </div>
        <div className="grid gap-3 md:justify-items-end">
          {userEmail && (
            <p className="m-0 font-mono text-[0.78rem] text-ink-muted">{userEmail}</p>
          )}
          <button
            className={secondaryButtonClass}
            type="button"
            onClick={onSignOut}
          >
            Sign out
          </button>
        </div>
      </div>

      <section
        className="mt-14 grid items-start gap-7 md:grid-cols-[minmax(0,1.25fr)_minmax(320px,0.75fr)]"
        aria-label="Organization onboarding"
      >
        <form
          className={cx(surfaceClass, "grid max-w-[760px] gap-[18px] p-[22px]")}
          onSubmit={(event) => {
            event.preventDefault();
            void handleCreateOrganization();
          }}
        >
          <div className={sectionLabelClass}>Create organization</div>

          <label className={fieldLabelClass} htmlFor="organization-name">
            Organization name
          </label>
          <input
            className={jumboInputClass}
            id="organization-name"
            name="organization-name"
            type="text"
            autoComplete="organization"
            autoFocus
            spellCheck={false}
            value={organizationName}
            onChange={(event) => setOrganizationName(event.target.value)}
          />

          {(error || createError) && (
            <p className={errorMessageClass}>{error || createError}</p>
          )}

          <div className="flex flex-wrap items-center gap-4">
            <button
              className={primaryButtonClass}
              type="submit"
              disabled={isCreating}
            >
              {isCreating ? "Creating..." : "Create organization"}
            </button>
          </div>
        </form>

        <section
          className={cx(surfaceClass, "grid gap-[18px] p-[22px]")}
          aria-labelledby="join-existing-organization"
        >
          <div className={sectionLabelClass}>Join existing organization</div>
          <h2
            className="m-0 max-w-[12ch] text-4xl font-semibold leading-none text-ink sm:text-5xl"
            id="join-existing-organization"
          >
            Join with an invite.
          </h2>
          <p className="max-w-[32rem] text-[1.05rem] leading-8 text-ink-dim">
            You can&apos;t join organizations directly. Use an email-bound
            invite link from your manager.
          </p>
          <p className={`${messageClass} max-w-[24rem]`}>
            Ask your manager for an invite link, then sign in with that email
            address.
          </p>
        </section>
      </section>
    </main>
  );
}

function slugifyOrganizationName(value: string) {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}
