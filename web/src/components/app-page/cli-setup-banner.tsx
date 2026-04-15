import { useNavigate } from "react-router-dom";

export function CliSetupBanner() {
  const navigate = useNavigate();

  return (
    <section className="workspace-banner">
      <div className="workspace-banner__body">
        <div className="section-label">CLI setup</div>
        <h2>Install and sign in to the CLI before repo activity lands here.</h2>
        <p className="message">
          Open the setup docs, run the install command on the repo machine, then
          authenticate and join a room from that checkout.
        </p>
      </div>

      <button
        className="secondary-button"
        type="button"
        onClick={() => navigate("/install")}
      >
        Open setup docs
      </button>
    </section>
  );
}
