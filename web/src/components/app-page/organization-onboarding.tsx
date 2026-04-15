import { useState } from "react";
import { authClient } from "../../auth-client";
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
    <main className="landing-page first-run-page">
      <div className="first-run-header">
        <div className="first-run-header__copy">
          <div className="eyebrow">supermanager</div>
          <h1>Choose how you want to start.</h1>
          <p className="hero-text first-run-header__text">
            Create your organization now, or join one through an invite link.
          </p>
        </div>
        <div className="first-run-header__actions">
          {userEmail && <p className="first-run-header__email">{userEmail}</p>}
          <button
            className="secondary-button"
            type="button"
            onClick={onSignOut}
          >
            Sign out
          </button>
        </div>
      </div>

      <section
        className="first-run-layout"
        aria-label="Organization onboarding"
      >
        <form
          className="first-run-column first-run-column--primary"
          onSubmit={(event) => {
            event.preventDefault();
            void handleCreateOrganization();
          }}
        >
          <div className="section-label">Create organization</div>

          <label
            className="create-room-dialog__label"
            htmlFor="organization-name"
          >
            Organization name
          </label>
          <input
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
            <p className="message message--error">{error || createError}</p>
          )}

          <div className="first-run-actions">
            <button
              className="primary-button"
              type="submit"
              disabled={isCreating}
            >
              {isCreating ? "Creating..." : "Create organization"}
            </button>
          </div>
        </form>

        <section
          className="first-run-column"
          aria-labelledby="join-existing-organization"
        >
          <div className="section-label">Join existing organization</div>
          <h2 id="join-existing-organization">Join with an invite.</h2>
          <p className="message first-run-copy">
            You can&apos;t join organizations directly. Use an email-bound
            invite link from your manager.
          </p>
          <p className="message first-run-hint">
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
