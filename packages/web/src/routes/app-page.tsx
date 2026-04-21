import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api } from "../api";
import { authClient } from "../auth-client";
import { CliSetupBanner } from "../components/app-page/cli-setup-banner";
import { CreateProjectDialog } from "../components/app-page/create-project-dialog";
import { DeviceApprovalDialog } from "../components/app-page/device-approval-dialog";
import { InviteTeammateDialog } from "../components/app-page/invite-teammate-dialog";
import { InviteJoinGate } from "../components/app-page/invite-join-gate";
import { InviteTeammatesBanner } from "../components/app-page/invite-teammates-banner";
import { OrganizationKnowledgePanel } from "../components/app-page/organization-knowledge-panel";
import { OrganizationMembersPanel } from "../components/app-page/organization-members-panel";
import { OrganizationOnboarding } from "../components/app-page/organization-onboarding";
import { OrgWideBlufCard } from "../components/app-page/org-wide-bluf-card";
import { WorkspaceHeader } from "../components/app-page/workspace-header";
import { WorkspacePanel } from "../components/app-page/workspace-panel";
import { InnerTabNav, type InnerTabItem } from "../components/inner-tab-nav";
import {
  buildOrganizationHref,
  buildOrganizationKnowledgeHref,
  buildOrganizationMembersHref,
} from "../lib/organization";
import {
  deviceStatusQueryKey,
  normalizeUserCode,
  useDeviceStatus,
} from "../queries/device-status";
import { useOrganizationDocuments } from "../queries/organization-documents";
import {
  organizationSummaryQueryRootKey,
  projectListQueryRootKey,
  useWorkspaceData,
  workspaceQueryKey,
} from "../queries/workspace";
import { pageShellClass } from "../ui";
import { readAuthError, readMessage } from "../utils";

export type AppPageView = "projects" | "members" | "knowledge";

interface AppPageProps {
  view?: AppPageView;
}

export function AppPage({ view = "projects" }: AppPageProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [workspaceActionError, setWorkspaceActionError] = useState<
    string | null
  >(null);
  const [createProjectError, setCreateProjectError] = useState<string | null>(null);
  const [deviceActionError, setDeviceActionError] = useState<string | null>(
    null,
  );
  const [pendingAction, setPendingAction] = useState<
    "sign-out" | "create-project" | null
  >(null);
  const [pendingDeviceAction, setPendingDeviceAction] = useState<
    "approve" | "deny" | null
  >(null);
  const [isCreateProjectDialogOpen, setIsCreateProjectDialogOpen] = useState(false);
  const [isInviteDialogOpen, setIsInviteDialogOpen] = useState(false);
  const [createProjectName, setCreateProjectName] = useState("");

  const searchParams = new URLSearchParams(location.search);
  const userCode = normalizeUserCode(searchParams.get("user_code"));
  const preferredOrganizationSlug = searchParams.get("organization");
  const {
    activeOrganization,
    projects,
    projectsQuery,
    summaryQuery,
    viewerQuery,
  } =
    useWorkspaceData(preferredOrganizationSlug);
  const deviceStatusQuery = useDeviceStatus(userCode);
  const organizationSlug = activeOrganization?.organization_slug ?? null;
  const isKnowledgeView = view === "knowledge";
  const memoriesQuery = useOrganizationDocuments(
    "memories",
    organizationSlug,
    isKnowledgeView,
  );
  const skillsQuery = useOrganizationDocuments(
    "skills",
    organizationSlug,
    isKnowledgeView,
  );

  const viewer = viewerQuery.data ?? null;
  const isFirstRun = viewer !== null && viewer.organizations.length === 0;
  const hasWorkspaceData = !activeOrganization || projectsQuery.data != null;
  const isLoading =
    viewerQuery.isLoading ||
    (Boolean(activeOrganization) &&
      !hasWorkspaceData &&
      projectsQuery.isLoading);
  const isKnowledgeLoading =
    isKnowledgeView &&
    (memoriesQuery.isLoading || skillsQuery.isLoading) &&
    !memoriesQuery.data &&
    !skillsQuery.data;
  const organizationSummary = summaryQuery.data?.summary ?? null;
  const summaryStatus = readSummaryStatus(summaryQuery.error, summaryQuery.data);
  const memberSnapshots = organizationSummary?.members ?? [];
  const workspaceError =
    workspaceActionError ||
    readQueryError(viewerQuery.error, viewerQuery.data != null) ||
    readQueryError(projectsQuery.error, projectsQuery.data != null) ||
    (isKnowledgeView
      ? readQueryError(memoriesQuery.error, memoriesQuery.data != null) ??
        readQueryError(skillsQuery.error, skillsQuery.data != null)
      : null);
  const deviceError =
    deviceActionError || readQueryError(deviceStatusQuery.error);
  const isCreatingProject = pendingAction === "create-project";

  function openDocs() {
    navigate("/docs");
  }

  async function handleSignOut() {
    setPendingAction("sign-out");
    setWorkspaceActionError(null);

    const result = await authClient.signOut();

    setPendingAction(null);
    if (result.error) {
      setWorkspaceActionError(readAuthError(result.error));
      return;
    }

    navigate("/login", { replace: true });
  }

  function openCreateProjectDialog() {
    setCreateProjectError(null);
    setCreateProjectName("");
    setIsCreateProjectDialogOpen(true);
  }

  function closeCreateProjectDialog() {
    if (isCreatingProject) {
      return;
    }

    setIsCreateProjectDialogOpen(false);
    setCreateProjectError(null);
    setCreateProjectName("");
  }

  async function handleCreateProjectSubmit() {
    if (!activeOrganization) {
      return;
    }

    const name = createProjectName.trim();
    if (!name) {
      setCreateProjectError("Project name is required.");
      return;
    }

    setPendingAction("create-project");
    setCreateProjectError(null);

    try {
      const project = await api.createProject({
        name,
        organizationSlug: activeOrganization.organization_slug,
      });

      await refreshWorkspace();
      setIsCreateProjectDialogOpen(false);
      setCreateProjectName("");
      navigate(`/p/${project.project_id}`);
    } catch (error) {
      setCreateProjectError(readMessage(error));
    } finally {
      setPendingAction(null);
    }
  }

  async function handleDeviceAction(action: "approve" | "deny") {
    if (!userCode) {
      return;
    }

    setPendingDeviceAction(action);
    setDeviceActionError(null);

    const result =
      action === "approve"
        ? await authClient.device.approve({ userCode })
        : await authClient.device.deny({ userCode });

    setPendingDeviceAction(null);

    if (result.error) {
      setDeviceActionError(readAuthError(result.error));
      return;
    }

    queryClient.setQueryData(
      deviceStatusQueryKey(userCode),
      action === "approve" ? "approved" : "denied",
    );
  }

  function closeDeviceDialog() {
    const params = new URLSearchParams(location.search);
    params.delete("user_code");
    const query = params.toString();
    navigate(query ? `${location.pathname}?${query}` : location.pathname, {
      replace: true,
    });
  }

  const refreshWorkspace = useCallback(async () => {
    await Promise.all([
      queryClient.invalidateQueries({
        queryKey: organizationSummaryQueryRootKey(),
      }),
      queryClient.invalidateQueries({ queryKey: workspaceQueryKey() }),
      queryClient.invalidateQueries({ queryKey: projectListQueryRootKey() }),
    ]);
    setPendingAction(null);
  }, [queryClient]);

  const knowledgeCount =
    (memoriesQuery.data?.documents.length ?? 0) +
    (skillsQuery.data?.documents.length ?? 0);
  const tabItems: Array<InnerTabItem<AppPageView>> = [
    {
      id: "projects",
      label: "Projects",
      to: buildOrganizationHref(organizationSlug),
      count: projects.length || undefined,
    },
    {
      id: "members",
      label: "Members",
      to: buildOrganizationMembersHref(organizationSlug),
      count: memberSnapshots.length || undefined,
    },
    {
      id: "knowledge",
      label: "Knowledge",
      to: buildOrganizationKnowledgeHref(organizationSlug),
      count: knowledgeCount || undefined,
    },
  ];

  return (
    <>
      <InviteJoinGate onRefreshWorkspace={refreshWorkspace} />

      {isFirstRun ? (
        <OrganizationOnboarding
          error={workspaceError}
          onRefreshWorkspace={refreshWorkspace}
          onSignOut={() => void handleSignOut()}
          userEmail={viewer?.user.email ?? null}
        />
      ) : (
        <main className={pageShellClass}>
          <WorkspaceHeader
            activeOrganizationName={
              activeOrganization?.organization_name ?? null
            }
            activeOrganizationSlug={organizationSlug}
            isSigningOut={pendingAction === "sign-out"}
            userEmail={viewer?.user.email ?? null}
            onInviteTeammate={() => setIsInviteDialogOpen(true)}
            onOpenDocs={openDocs}
            onSignOut={() => void handleSignOut()}
          />

          {activeOrganization && !isLoading ? (
            <div className="mt-7">
              <OrgWideBlufCard
                organizationSummary={organizationSummary}
                showStatusMeta
                summaryStatus={summaryStatus}
              />
            </div>
          ) : null}

          <InnerTabNav
            activeId={view}
            ariaLabel="Organization sections"
            items={tabItems}
          />

          {activeOrganization && activeOrganization.member_count <= 1 && (
            <InviteTeammatesBanner
              onInviteTeammate={() => setIsInviteDialogOpen(true)}
            />
          )}

          {view === "projects" && activeOrganization && viewer && !viewer.has_cli_auth && (
            <CliSetupBanner />
          )}

          {view === "members" ? (
            <OrganizationMembersPanel
              activeOrganization={activeOrganization}
              error={workspaceError}
              isLoading={isLoading}
              members={memberSnapshots}
              organizationSlug={organizationSlug}
            />
          ) : view === "knowledge" ? (
            <OrganizationKnowledgePanel
              activeOrganization={activeOrganization}
              error={workspaceError}
              isLoading={isLoading || isKnowledgeLoading}
              memoriesResponse={memoriesQuery.data ?? null}
              skillsResponse={skillsQuery.data ?? null}
            />
          ) : (
            <div className="mt-7">
              <WorkspacePanel
                activeOrganization={activeOrganization}
                error={workspaceError}
                isCreatingProject={isCreatingProject}
                isLoading={isLoading}
                projects={projects}
                onCreateProject={openCreateProjectDialog}
              />
            </div>
          )}
        </main>
      )}

      {isCreateProjectDialogOpen && (
        <CreateProjectDialog
          error={createProjectError}
          isCreating={isCreatingProject}
          name={createProjectName}
          onClose={closeCreateProjectDialog}
          onCreate={() => void handleCreateProjectSubmit()}
          onNameChange={setCreateProjectName}
        />
      )}

      {isInviteDialogOpen && activeOrganization && (
        <InviteTeammateDialog
          organizationId={activeOrganization.organization_id}
          organizationName={activeOrganization.organization_name}
          onClose={() => setIsInviteDialogOpen(false)}
        />
      )}

      {userCode && (
        <DeviceApprovalDialog
          error={deviceError}
          pendingAction={pendingDeviceAction}
          status={deviceStatusQuery.data ?? null}
          userCode={userCode}
          onApprove={() => void handleDeviceAction("approve")}
          onClose={closeDeviceDialog}
          onDeny={() => void handleDeviceAction("deny")}
        />
      )}
    </>
  );
}

function readQueryError(error: unknown, hasData: boolean = false) {
  if (hasData) {
    return null;
  }

  return error instanceof Error ? error.message : null;
}

function readSummaryStatus(
  error: unknown,
  data: { status: "generating" | "ready" | "error" } | undefined,
) {
  if (data) {
    return data.status;
  }

  if (error instanceof Error) {
    return "error" as const;
  }

  return "ready" as const;
}
