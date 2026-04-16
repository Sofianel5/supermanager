import { describe, expect, it } from "bun:test";

import type { OrganizationMembership } from "./types";
import { pickActiveOrganizationMembership } from "./middleware";

describe("pickActiveOrganizationMembership", () => {
  it("prefers the active organization when present", () => {
    const memberships = [
      membership("org-1", "alpha"),
      membership("org-2", "beta"),
    ];

    expect(
      pickActiveOrganizationMembership(memberships, "org-2")?.organization_slug,
    ).toBe("beta");
  });

  it("falls back to the only organization when no active org is set", () => {
    const memberships = [membership("org-1", "alpha")];

    expect(
      pickActiveOrganizationMembership(memberships, null)?.organization_slug,
    ).toBe("alpha");
  });

  it("does not guess when multiple organizations are available", () => {
    const memberships = [
      membership("org-1", "alpha"),
      membership("org-2", "beta"),
    ];

    expect(pickActiveOrganizationMembership(memberships, null)).toBeNull();
  });
});

function membership(
  organizationId: string,
  organizationSlug: string,
): OrganizationMembership {
  return {
    organization_id: organizationId,
    organization_name: organizationSlug.toUpperCase(),
    organization_slug: organizationSlug,
    member_count: 1,
    role: "member",
  };
}
