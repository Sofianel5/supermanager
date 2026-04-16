import type { SupermanagerAuth } from "./auth";
import { Db } from "./db";
import type { OrganizationMembership } from "./types";

export async function requireViewer(auth: SupermanagerAuth, headers: Headers) {
  const session = await auth.api.getSession({ headers });
  if (!session) {
    throw httpError(401, "authentication required");
  }
  return session;
}

export async function resolveOrganizationMembership(
  db: Db,
  userId: string,
  organizationSlug: string | undefined,
  activeOrganizationId: string | null,
) {
  if (organizationSlug) {
    const membership = await db.getOrganizationMembershipBySlug(userId, organizationSlug);
    if (!membership) {
      throw httpError(403, `organization not available: ${organizationSlug}`);
    }
    return membership;
  }

  const memberships = await db.listOrganizationsForUser(userId);
  const membership = pickActiveOrganizationMembership(
    memberships,
    activeOrganizationId,
  );
  if (membership) {
    return membership;
  }

  throw httpError(400, "select an organization first");
}

export function pickActiveOrganizationMembership(
  memberships: OrganizationMembership[],
  activeOrganizationId: string | null,
) {
  if (activeOrganizationId) {
    const activeMembership = memberships.find(
      (membership) => membership.organization_id === activeOrganizationId,
    );
    if (activeMembership) {
      return activeMembership;
    }
  }

  if (memberships.length === 1) {
    return memberships[0];
  }

  return null;
}

export async function requireRoomAccess(db: Db, userId: string, roomId: string) {
  const room = await db.getRoomWithAccessCheck(roomId, userId);
  if (!room) {
    throw httpError(404, `room not found: ${roomId}`);
  }

  return room;
}

export function httpError(statusCode: number, message: string) {
  return Object.assign(new Error(message), { status: statusCode });
}

export function readRequiredHeader(headers: Headers, name: string) {
  const value = headers.get(name)?.trim();
  if (!value) {
    throw httpError(401, `missing ${name}`);
  }
  return value;
}
