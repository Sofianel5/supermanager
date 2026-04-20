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

export async function requireProjectAccess(db: Db, userId: string, projectId: string) {
  const project = await db.getProjectWithAccessCheck(projectId, userId);
  if (!project) {
    throw httpError(404, `project not found: ${projectId}`);
  }

  return project;
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
