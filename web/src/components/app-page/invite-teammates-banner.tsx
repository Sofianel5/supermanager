interface InviteTeammatesBannerProps {
  onInviteTeammate(): void;
}

export function InviteTeammatesBanner({
  onInviteTeammate,
}: InviteTeammatesBannerProps) {
  return (
    <section className="workspace-banner">
      <div className="workspace-banner__body">
        <div className="section-label">Team setup</div>
        <h2>Add teammates so this workspace is shared.</h2>
        <p className="message">
          Invite the rest of the organization so rooms and updates reach the people
          who need them.
        </p>
      </div>

      <button
        className="secondary-button"
        type="button"
        onClick={onInviteTeammate}
      >
        Invite teammate
      </button>
    </section>
  );
}
