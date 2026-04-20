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
  showStatusMeta?: boolean;
  summaryStatus: SummaryStatus;
}

export function OrgWideBlufCard({
  action,
  organizationSummary,
  showStatusMeta = false,
  summaryStatus,
}: OrgWideBlufCardProps) {
  const hasBluf = Boolean(organizationSummary?.bluf_markdown.trim());

  return (
    <section className={cx(accentSurfaceClass, "grid gap-4 p-[18px]")}>
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <span className={sectionLabelClass}>Organization summary</span>
        {showStatusMeta ? (
          <div className="flex flex-wrap gap-2">
            <span className={cx(pillBaseClass, summaryToneClass(summaryStatus))}>
              {summaryStatusLabel(summaryStatus)}
            </span>
            <span className={`${pillBaseClass} border-border text-ink-dim`}>
              Refreshes every 5 min
            </span>
          </div>
        ) : null}
      </div>

      {hasBluf ? (
        <MarkdownBlock markdown={organizationSummary!.bluf_markdown} />
      ) : (
        <p className={messageClass}>
          No organization summary yet. New hook activity will build it here.
        </p>
      )}

      {action ? <div className="flex justify-end">{action}</div> : null}
    </section>
  );
}

function summaryStatusLabel(status: SummaryStatus) {
  switch (status) {
    case "generating":
      return "Refreshing";
    case "error":
      return "Error";
    default:
      return "Ready";
  }
}

function summaryToneClass(status: SummaryStatus) {
  switch (status) {
    case "generating":
      return "border-accent/30 text-accent";
    case "error":
      return "border-red-400/30 text-danger";
    default:
      return "border-emerald-400/30 text-success";
  }
}
