import {
  accentSurfaceClass,
  cx,
  messageClass,
  sectionLabelClass,
} from "../../ui";
import { SecondaryActionLink } from "./secondary-action-link";

export function CliSetupBanner() {
  return (
    <section
      className={cx(
        accentSurfaceClass,
        "mt-6 flex flex-col gap-5 p-[22px] md:flex-row md:items-end md:justify-between",
      )}
    >
      <div className="grid gap-2.5">
        <div className={sectionLabelClass}>CLI setup</div>
        <h2 className="m-0 max-w-[24ch] text-3xl font-semibold leading-none text-ink sm:text-[2.6rem]">
          Your sessions aren't streaming yet.
        </h2>
        <p className={messageClass}>
          Install the Supermanager CLI and sign in to start sending your Claude
          Code and Codex activity to your team.
        </p>
      </div>

      <SecondaryActionLink to="/docs">Open setup docs</SecondaryActionLink>
    </section>
  );
}
