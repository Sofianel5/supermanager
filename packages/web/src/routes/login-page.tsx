import { useState } from "react";
import { Link, Navigate, useLocation } from "react-router-dom";
import { authClient, sanitizeReturnTo, toAbsoluteCallbackUrl } from "../auth-client";
import { normalizeUserCode } from "../queries/device-status";
import {
  centeredShellClass,
  errorMessageClass,
  messageClass,
  secondaryButtonClass,
  sectionLabelClass,
  statusBlockClass,
} from "../ui";
import { readAuthError, readMessage } from "../utils";

type SocialProvider = "github" | "google";

export function LoginPage() {
  const location = useLocation();
  const session = authClient.useSession();
  const [error, setError] = useState<string | null>(null);
  const [pendingProvider, setPendingProvider] = useState<SocialProvider | null>(null);

  const searchParams = new URLSearchParams(location.search);
  const userCode = normalizeUserCode(searchParams.get("user_code"));
  const sanitizedReturnTo = sanitizeReturnTo(searchParams.get("returnTo"));
  const returnTo = sanitizedReturnTo === "/login" ? "/app" : sanitizedReturnTo;
  const loginPath = buildLoginPath(returnTo, userCode);
  const callbackPath = userCode
    ? `/app?${new URLSearchParams({ user_code: userCode }).toString()}`
    : returnTo;

  async function signIn(provider: SocialProvider) {
    if (pendingProvider) {
      return;
    }

    setError(null);
    setPendingProvider(provider);

    try {
      const result = await authClient.signIn.social({
        callbackURL: toAbsoluteCallbackUrl(callbackPath),
        errorCallbackURL: toAbsoluteCallbackUrl(loginPath),
        provider,
      });

      if (result.error) {
        setError(readAuthError(result.error));
      }
    } catch (error) {
      const message = readMessage(error);
      setError(
        /fetch/i.test(message)
          ? "Couldn't reach the sign-in service. Try again in a moment."
          : message,
      );
    } finally {
      setPendingProvider(null);
    }
  }

  if (session.isPending) {
    return (
      <main className={centeredShellClass}>
        <div className={statusBlockClass}>
          <span className={sectionLabelClass}>supermanager</span>
          <h1 className="mt-4 text-4xl font-semibold leading-none text-ink sm:text-5xl">
            Checking your session…
          </h1>
        </div>
      </main>
    );
  }

  if (session.data) {
    return <Navigate replace to={callbackPath} />;
  }

  return (
    <main className={centeredShellClass}>
      <section className={`${statusBlockClass} grid w-full max-w-[420px] gap-[18px]`}>
        <div>
          <div className={sectionLabelClass}>supermanager</div>
          <h1 className="mt-4 text-[clamp(2.8rem,8vw,4.6rem)] font-bold leading-none tracking-[-0.04em] text-ink">
            Sign in
          </h1>
          <p className={`${messageClass} mt-4`}>
            Continue with Google or GitHub to manage projects and approve CLI logins.
          </p>
        </div>

        <div className="mt-[18px] grid gap-3">
          <button
            className={`${secondaryButtonClass} w-full`}
            type="button"
            disabled={pendingProvider !== null}
            onClick={() => void signIn("google")}
          >
            {pendingProvider === "google" ? "Connecting to Google..." : "Continue with Google"}
          </button>
          <button
            className={`${secondaryButtonClass} w-full`}
            type="button"
            disabled={pendingProvider !== null}
            onClick={() => void signIn("github")}
          >
            {pendingProvider === "github" ? "Connecting to GitHub..." : "Continue with GitHub"}
          </button>
        </div>
        {error && <p className={errorMessageClass}>{error}</p>}
        <Link className={secondaryButtonClass} to="/">
          Back
        </Link>
      </section>
    </main>
  );
}

function buildLoginPath(returnTo: string, userCode: string) {
  const params = new URLSearchParams();
  if (userCode) {
    params.set("user_code", userCode);
  }
  if (returnTo !== "/app") {
    params.set("returnTo", returnTo);
  }
  const query = params.toString();
  return query ? `/login?${query}` : "/login";
}
