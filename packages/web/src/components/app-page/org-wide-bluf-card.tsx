import type { ReactNode } from "react";
import type { OrganizationSnapshot, SummaryStatus } from "../../api";
import {
  accentSurfaceClass,
  cx,
  messageClass,
  pillBaseClass,
  sectionLabelClass,
} from "../../ui";
import { MarkdownBlock } from "../markdown-block";

interface OrgWideBlufCardProps {
  action?: ReactNode;
  organizationSummary: OrganizationSnapshot | null;
  summaryStatus: SummaryStatus;
}

export function OrgWideBlufCard({
  action,
  organizationSummary,
  summaryStatus,
}: OrgWideBlufCardProps) {
  const hasBluf = Boolean(organizationSummary?.bluf_markdown.trim());

  return (
    <section className={cx(accentSurfaceClass, "grid gap-4 p-[18px]")}>
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <span className={sectionLabelClass}>Org-wide BLUF</span>
        <span className={cx(pillBaseClass, summaryToneClass(summaryStatus))}>
          {summaryStatus}
        </span>
      </div>

      {hasBluf ? (
        <MarkdownBlock markdown={organizationSummary!.bluf_markdown} />
      ) : (
        <p className={messageClass}>
          No organization BLUF yet. New hook activity will build it here.
        </p>
      )}

      {action ? <div className="flex justify-end">{action}</div> : null}
    </section>
  );
}

function summaryToneClass(status: SummaryStatus) {
  switch (status) {
    case "generating":
      return "border-amber-400/40 bg-amber-400/12 text-amber-100";
    case "error":
      return "border-red-400/40 bg-red-400/12 text-red-100";
    default:
      return "border-emerald-400/35 bg-emerald-400/12 text-emerald-100";
  }
}
