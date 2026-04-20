import {
  accentSurfaceClass,
  cx,
  messageClass,
  secondaryButtonClass,
  sectionLabelClass,
} from "../../ui";

interface InviteTeammatesBannerProps {
  onInviteTeammate(): void;
}

export function InviteTeammatesBanner({
  onInviteTeammate,
}: InviteTeammatesBannerProps) {
  return (
    <section
      className={cx(
        accentSurfaceClass,
        "mt-6 flex flex-col gap-5 p-[22px] md:flex-row md:items-end md:justify-between",
      )}
    >
      <div className="grid gap-2.5">
        <div className={sectionLabelClass}>Team setup</div>
        <h2 className="m-0 max-w-[24ch] text-3xl font-semibold leading-none text-ink sm:text-[2.6rem]">
          Add teammates so this workspace is shared.
        </h2>
        <p className={messageClass}>
          Invite the rest of the organization so projects and updates reach the people
          who need them.
        </p>
      </div>

      <button
        className={secondaryButtonClass}
        type="button"
        onClick={onInviteTeammate}
      >
        Invite teammate
      </button>
    </section>
  );
}
