import type { SupermanagerAuth } from "./auth";
import { Db } from "./db";

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

  if (activeOrganizationId) {
    const membership = await db.getOrganizationMembershipById(userId, activeOrganizationId);
    if (membership) {
      return membership;
    }
  }

  throw httpError(400, "select an organization first");
}

export async function requireRoomAccess(db: Db, userId: string, roomId: string) {
  const room = await db.getRoom(roomId);
  if (!room) {
    throw httpError(404, `room not found: ${roomId}`);
  }

  const membership = await db.getOrganizationMembershipById(userId, room.organization_id);
  if (!membership) {
    throw httpError(403, "forbidden");
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
