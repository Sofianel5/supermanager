import type { ViewerOrganization } from "../../api";
import { CopyPanel } from "../copy-panel";

const INSTALL_COMMAND = "curl -fsSL https://supermanager.dev/install.sh | sh";
const LOGIN_COMMAND = "supermanager login";

interface CliPanelProps {
  activeOrganization: ViewerOrganization | null;
  copiedValue: string | null;
  onCopy(label: string, value: string): Promise<void>;
}

export function CliPanel({ activeOrganization, copiedValue, onCopy }: CliPanelProps) {
  return (
    <div className="landing-column landing-column--form">
      <div className="section-label">CLI</div>
      <p className="message">
        Keep setup human-first in the browser, then do the repo work from the
        terminal.
      </p>

      <CopyPanel
        copiedValue={copiedValue}
        label="Install CLI"
        onCopy={onCopy}
        value={INSTALL_COMMAND}
      />
      <CopyPanel
        copiedValue={copiedValue}
        label="Login"
        onCopy={onCopy}
        value={LOGIN_COMMAND}
      />

      {activeOrganization && (
        <>
          <CopyPanel
            copiedValue={copiedValue}
            label="Create room"
            onCopy={onCopy}
            value={`supermanager create room --org "${activeOrganization.organization_slug}"`}
          />
          <CopyPanel
            copiedValue={copiedValue}
            label="Join repo"
            onCopy={onCopy}
            value={`supermanager join ROOM_ID --org "${activeOrganization.organization_slug}"`}
          />
        </>
      )}
    </div>
  );
}
