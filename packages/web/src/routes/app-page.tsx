import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api } from "../api";
import { authClient } from "../auth-client";
import { CliSetupBanner } from "../components/app-page/cli-setup-banner";
import { CreateRoomDialog } from "../components/app-page/create-room-dialog";
import { DeviceApprovalDialog } from "../components/app-page/device-approval-dialog";
import { InviteTeammateDialog } from "../components/app-page/invite-teammate-dialog";
import { InviteJoinGate } from "../components/app-page/invite-join-gate";
import { InviteTeammatesBanner } from "../components/app-page/invite-teammates-banner";
import { OrgWideBlufCard } from "../components/app-page/org-wide-bluf-card";
import { OrganizationInsightsHeader } from "../components/app-page/organization-insights-header";
import { OrganizationInsightsPanel } from "../components/app-page/organization-insights-panel";
import { OrganizationOnboarding } from "../components/app-page/organization-onboarding";
import { SecondaryActionLink } from "../components/app-page/secondary-action-link";
import { WorkspaceHeader } from "../components/app-page/workspace-header";
import { WorkspacePanel } from "../components/app-page/workspace-panel";
import {
  deviceStatusQueryKey,
  normalizeUserCode,
  useDeviceStatus,
} from "../queries/device-status";
import {
  organizationSummaryQueryRootKey,
  roomListQueryRootKey,
  useWorkspaceData,
  workspaceQueryKey,
} from "../queries/workspace";
import { pageShellClass } from "../ui";
import { readAuthError, readMessage } from "../utils";

interface AppPageProps {
  view?: "rooms" | "insights";
}

export function AppPage({ view = "rooms" }: AppPageProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [workspaceActionError, setWorkspaceActionError] = useState<
    string | null
  >(null);
  const [createRoomError, setCreateRoomError] = useState<string | null>(null);
  const [deviceActionError, setDeviceActionError] = useState<string | null>(
    null,
  );
  const [pendingAction, setPendingAction] = useState<
    "sign-out" | "create-room" | null
  >(null);
  const [pendingDeviceAction, setPendingDeviceAction] = useState<
    "approve" | "deny" | null
  >(null);
  const [isCreateRoomDialogOpen, setIsCreateRoomDialogOpen] = useState(false);
  const [isInviteDialogOpen, setIsInviteDialogOpen] = useState(false);
  const [createRoomName, setCreateRoomName] = useState("");

  const searchParams = new URLSearchParams(location.search);
  const userCode = normalizeUserCode(searchParams.get("user_code"));
  const preferredOrganizationSlug = searchParams.get("organization");
  const { activeOrganization, rooms, roomsQuery, summaryQuery, viewerQuery } =
    useWorkspaceData(preferredOrganizationSlug);
  const deviceStatusQuery = useDeviceStatus(userCode);

  const viewer = viewerQuery.data ?? null;
  const isFirstRun = viewer !== null && viewer.organizations.length === 0;
  const hasWorkspaceData = !activeOrganization || roomsQuery.data != null;
  const isLoading =
    viewerQuery.isLoading ||
    (Boolean(activeOrganization) &&
      !hasWorkspaceData &&
      roomsQuery.isLoading);
  const workspaceError =
    workspaceActionError ||
    readQueryError(viewerQuery.error, viewerQuery.data != null) ||
    readQueryError(roomsQuery.error, roomsQuery.data != null);
  const deviceError =
    deviceActionError || readQueryError(deviceStatusQuery.error);
  const isCreatingRoom = pendingAction === "create-room";
  const summaryStatus = readSummaryStatus(summaryQuery.error, summaryQuery.data);

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

  function openCreateRoomDialog() {
    setCreateRoomError(null);
    setCreateRoomName("");
    setIsCreateRoomDialogOpen(true);
  }

  function closeCreateRoomDialog() {
    if (isCreatingRoom) {
      return;
    }

    setIsCreateRoomDialogOpen(false);
    setCreateRoomError(null);
    setCreateRoomName("");
  }

  async function handleCreateRoomSubmit() {
    if (!activeOrganization) {
      return;
    }

    const name = createRoomName.trim();
    if (!name) {
      setCreateRoomError("Room name is required.");
      return;
    }

    setPendingAction("create-room");
    setCreateRoomError(null);

    try {
      const room = await api.createRoom({
        name,
        organizationSlug: activeOrganization.organization_slug,
      });

      await refreshWorkspace();
      setIsCreateRoomDialogOpen(false);
      setCreateRoomName("");
      navigate(`/r/${room.room_id}`);
    } catch (error) {
      setCreateRoomError(readMessage(error));
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
      queryClient.invalidateQueries({ queryKey: roomListQueryRootKey() }),
    ]);
    setPendingAction(null);
  }, [queryClient]);

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
          {view === "insights" ? (
            <OrganizationInsightsHeader
              isSigningOut={pendingAction === "sign-out"}
              organizationName={activeOrganization?.organization_name ?? null}
              organizationSlug={activeOrganization?.organization_slug ?? null}
              organizationSummary={summaryQuery.data?.summary ?? null}
              organizationSummaryUpdatedAt={summaryQuery.data?.updated_at ?? null}
              onInviteTeammate={() => setIsInviteDialogOpen(true)}
              onOpenDocs={openDocs}
              onSignOut={() => void handleSignOut()}
              summaryStatus={summaryStatus}
            />
          ) : (
            <WorkspaceHeader
              activeOrganizationName={
                activeOrganization?.organization_name ?? null
              }
              activeOrganizationSlug={
                activeOrganization?.organization_slug ?? null
              }
              isSigningOut={pendingAction === "sign-out"}
              userEmail={viewer?.user.email ?? null}
              onInviteTeammate={() => setIsInviteDialogOpen(true)}
              onOpenDocs={openDocs}
              onSignOut={() => void handleSignOut()}
            />
          )}

          {activeOrganization && activeOrganization.member_count <= 1 && (
            <InviteTeammatesBanner
              onInviteTeammate={() => setIsInviteDialogOpen(true)}
            />
          )}

          {view === "rooms" && activeOrganization && viewer && !viewer.has_cli_auth && (
            <CliSetupBanner />
          )}

          {view === "insights" ? (
            <OrganizationInsightsPanel
              activeOrganization={activeOrganization}
              error={workspaceError}
              isLoading={isLoading}
              organizationSummary={summaryQuery.data?.summary ?? null}
              rooms={rooms}
              summaryStatus={summaryStatus}
            />
          ) : (
            <div className="mt-7 grid gap-6">
              {activeOrganization && !isLoading ? (
                <OrgWideBlufCard
                  action={
                    <SecondaryActionLink
                      to={buildOrganizationInsightsHref(activeOrganization.organization_slug)}
                    >
                      View org insights
                    </SecondaryActionLink>
                  }
                  organizationSummary={summaryQuery.data?.summary ?? null}
                  summaryStatus={summaryStatus}
                />
              ) : null}

              <WorkspacePanel
                activeOrganization={activeOrganization}
                error={workspaceError}
                isCreatingRoom={isCreatingRoom}
                isLoading={isLoading}
                rooms={rooms}
                onCreateRoom={openCreateRoomDialog}
              />
            </div>
          )}
        </main>
      )}

      {isCreateRoomDialogOpen && (
        <CreateRoomDialog
          error={createRoomError}
          isCreating={isCreatingRoom}
          name={createRoomName}
          onClose={closeCreateRoomDialog}
          onCreate={() => void handleCreateRoomSubmit()}
          onNameChange={setCreateRoomName}
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

function buildOrganizationInsightsHref(organizationSlug: string | null) {
  if (!organizationSlug) {
    return "/app/insights";
  }

  return `/app/insights?organization=${encodeURIComponent(organizationSlug)}`;
}
