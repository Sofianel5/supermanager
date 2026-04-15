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
import { WorkspaceHeader } from "../components/app-page/workspace-header";
import { WorkspacePanel } from "../components/app-page/workspace-panel";
import {
  deviceStatusQueryKey,
  normalizeUserCode,
  useDeviceStatus,
} from "../queries/device-status";
import {
  roomListQueryRootKey,
  useWorkspaceData,
  workspaceQueryKey,
} from "../queries/workspace";
import { readAuthError, readMessage } from "../utils";

export function AppPage() {
  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [workspaceActionError, setWorkspaceActionError] = useState<string | null>(null);
  const [createRoomError, setCreateRoomError] = useState<string | null>(null);
  const [deviceActionError, setDeviceActionError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<"sign-out" | "create-room" | null>(null);
  const [pendingDeviceAction, setPendingDeviceAction] = useState<"approve" | "deny" | null>(null);
  const [isCreateRoomDialogOpen, setIsCreateRoomDialogOpen] = useState(false);
  const [isInviteDialogOpen, setIsInviteDialogOpen] = useState(false);
  const [createRoomName, setCreateRoomName] = useState("");

  const userCode = normalizeUserCode(new URLSearchParams(location.search).get("user_code"));
  const { activeOrganization, rooms, roomsQuery, viewerQuery } = useWorkspaceData(null);
  const deviceStatusQuery = useDeviceStatus(userCode);

  const viewer = viewerQuery.data ?? null;
  const isLoading =
    viewerQuery.isLoading ||
    (Boolean(activeOrganization) && roomsQuery.isLoading);
  const workspaceError =
    workspaceActionError ||
    readQueryError(viewerQuery.error) ||
    readQueryError(roomsQuery.error);
  const deviceError =
    deviceActionError || readQueryError(deviceStatusQuery.error);
  const isCreatingRoom = pendingAction === "create-room";

  function openInstallInstructions() {
    navigate("/install");
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
    navigate(query ? `/app?${query}` : "/app", { replace: true });
  }

  const refreshWorkspace = useCallback(async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: workspaceQueryKey() }),
      queryClient.invalidateQueries({ queryKey: roomListQueryRootKey() }),
    ]);
    setPendingAction(null);
  }, [queryClient]);

  return (
    <>
      <InviteJoinGate onRefreshWorkspace={refreshWorkspace} />

      <main className="landing-page">
        <WorkspaceHeader
          activeOrganizationName={activeOrganization?.organization_name ?? null}
          activeOrganizationSlug={activeOrganization?.organization_slug ?? null}
          isSigningOut={pendingAction === "sign-out"}
          userEmail={viewer?.user.email ?? null}
          onInviteTeammate={() => setIsInviteDialogOpen(true)}
          onOpenInstallInstructions={openInstallInstructions}
          onSignOut={() => void handleSignOut()}
        />

        {activeOrganization && viewer && !viewer.has_cli_auth && (
          <CliSetupBanner />
        )}

        <WorkspacePanel
          activeOrganization={activeOrganization}
          error={workspaceError}
          isCreatingRoom={isCreatingRoom}
          isLoading={isLoading}
          rooms={rooms}
          viewer={viewer}
          onCreateRoom={openCreateRoomDialog}
        />
      </main>

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

function readQueryError(error: unknown) {
  return error instanceof Error ? error.message : null;
}
