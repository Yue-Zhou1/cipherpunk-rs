import {
  confirmWorkspace,
  createAuditSession,
  type ConfirmWorkspaceRequest,
} from "../../ipc/commands";

export type SessionFlowResult = {
  auditId: string;
  sessionId: string;
  snapshotId: string;
};

export async function createSessionFromConfirmation(
  request: ConfirmWorkspaceRequest
): Promise<SessionFlowResult> {
  const confirmation = await confirmWorkspace(request);
  let session;
  try {
    session = await createAuditSession();
  } catch (firstError) {
    try {
      session = await createAuditSession();
    } catch {
      const message = firstError instanceof Error ? firstError.message : String(firstError);
      throw new Error(
        `Workspace confirmed (${confirmation.auditId}) but session creation failed: ${message}`
      );
    }
  }

  return {
    auditId: confirmation.auditId,
    sessionId: session.sessionId,
    snapshotId: session.snapshotId,
  };
}
