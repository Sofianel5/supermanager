import { accentSurfaceClass, cx, messageClass, sectionLabelClass } from "../../ui";
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
          Install and sign in to the CLI before repo activity lands here.
        </h2>
        <p className={messageClass}>
          Open the setup docs, run the install command on the repo machine, then
          authenticate and join a room from that checkout.
        </p>
      </div>

      <SecondaryActionLink to="/docs">
        Open setup docs
      </SecondaryActionLink>
    </section>
  );
}
