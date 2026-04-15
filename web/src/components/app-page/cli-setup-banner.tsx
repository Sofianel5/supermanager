import { useNavigate } from "react-router-dom";
import { messageClass, secondaryButtonClass, sectionLabelClass } from "../../ui";

export function CliSetupBanner() {
  const navigate = useNavigate();

  return (
    <section className="mt-6 flex flex-col gap-5 border border-border-strong bg-[linear-gradient(180deg,rgba(245,158,11,0.12),rgba(8,12,19,0.9))] p-[22px] md:flex-row md:items-end md:justify-between">
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

      <button
        className={secondaryButtonClass}
        type="button"
        onClick={() => navigate("/docs")}
      >
        Open setup docs
      </button>
    </section>
  );
}
