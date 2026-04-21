import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link, useLocation, useParams } from "react-router-dom";
import type { MemberSnapshot, ProjectListEntry } from "../api";
import { ActivityUpdateList } from "../components/activity-update-list";
import { InnerTabNav, type InnerTabItem } from "../components/inner-tab-nav";
import { MemberAvatar } from "../components/member-avatar";
import { MarkdownBlock } from "../components/markdown-block";
import { displayMemberName } from "../lib/display-member-name";
import { formatRelativeTime } from "../lib/format-relative-time";
import {
  buildMemberActivityHref,
  buildMemberHref,
  buildOrganizationHref,
  formatOrganizationLabel,
} from "../lib/organization";
import { ACTIVITY_LIMIT, memberUpdatesQueryOptions } from "../queries/activity";
import { useWorkspaceData } from "../queries/workspace";
import {
  accentSurfaceClass,
  cx,
  messageClass,
  pageShellClass,
  projectMetaClass,
  secondaryButtonClass,
  sectionLabelClass,
  subduedSurfaceClass,
} from "../ui";

export type MemberPageView = "overview" | "activity";

interface MemberPageProps {
  view?: MemberPageView;
}

export function MemberPage({ view = "overview" }: MemberPageProps) {
  const { memberId = "" } = useParams();
  const location = useLocation();
  const [clock, setClock] = useState(() => Date.now());

  const searchParams = new URLSearchParams(location.search);
  const preferredOrganizationSlug = searchParams.get("organization");
  const { activeOrganization, projects, summaryQuery } = useWorkspaceData(
    preferredOrganizationSlug,
  );

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClock(Date.now());
    }, 30_000);

    return () => {
      window.clearInterval(timer);
    };
  }, []);

  const organizationSlug = activeOrganization?.organization_slug ?? null;
  const isActivityView = view === "activity";
  const activityQuery = useQuery({
    enabled: Boolean(organizationSlug) && Boolean(memberId) && isActivityView,
    ...memberUpdatesQueryOptions(
      organizationSlug ?? "",
      memberId,
      ACTIVITY_LIMIT,
    ),
    refetchInterval: 15_000,
    staleTime: 15_000,
  });
  const organizationHref = buildOrganizationHref(organizationSlug);
  const organizationLabel = formatOrganizationLabel(
    activeOrganization?.organization_name ?? null,
    organizationSlug,
  );
  const members = summaryQuery.data?.summary.members ?? [];
  const member = members.find((entry) => entry.member_user_id === memberId) ?? null;
  const memberProjects = member
    ? projects.filter((project) => member.project_ids.includes(project.project_id))
    : [];

  if (!summaryQuery.data && summaryQuery.isPending) {
    return (
      <main className={cx(pageShellClass, "grid min-h-[60vh] content-center gap-3")}>
        <div className={sectionLabelClass}>Member</div>
        <p className={messageClass}>Loading member summary...</p>
      </main>
    );
  }

  if (!member) {
    return (
      <main className={cx(pageShellClass, "grid min-h-[60vh] content-center gap-3")}>
        <div className={sectionLabelClass}>Member</div>
        <h1 className="m-0 text-4xl font-semibold leading-none text-ink sm:text-5xl">
          Unknown member
        </h1>
        <p className={messageClass}>
          This teammate doesn't have a summary yet or they aren't part of this
          organization.
        </p>
        <Link className={secondaryButtonClass} to={organizationHref}>
          Back to {organizationLabel}
        </Link>
      </main>
    );
  }

  const memberName = displayMemberName(member.member_name);
  const hasBluf = Boolean(member.bluf_markdown.trim());
  const tabItems: Array<InnerTabItem<MemberPageView>> = [
    {
      id: "overview",
      label: "Overview",
      to: buildMemberHref(member.member_user_id, organizationSlug),
      count: memberProjects.length || undefined,
    },
    {
      id: "activity",
      label: "Activity",
      to: buildMemberActivityHref(member.member_user_id, organizationSlug),
    },
  ];
  const activityError =
    activityQuery.error instanceof Error ? activityQuery.error.message : null;

  return (
    <main className={pageShellClass}>
      <header className="flex flex-col gap-7 border-b border-border pb-9 pt-7">
        <Link
          className="group inline-flex max-w-fit flex-wrap items-center gap-3 text-base font-medium text-ink no-underline transition hover:text-white"
          to={organizationHref}
        >
          <span className="font-mono text-[0.72rem] font-semibold uppercase tracking-[0.12em] text-accent transition-transform duration-150 group-hover:-translate-x-px">
            &lt;
          </span>
          <span>{`Back to ${organizationLabel}`}</span>
        </Link>
        <div className="flex flex-col gap-5 md:flex-row md:items-center md:gap-6">
          <MemberAvatar name={memberName} size="lg" />
          <div className="flex min-w-0 flex-col gap-2">
            <div className={sectionLabelClass}>Member</div>
            <h1 className="m-0 max-w-full text-4xl font-semibold leading-none text-ink sm:text-5xl">
              {memberName}
            </h1>
            <p className={projectMetaClass}>
              <span>{member.project_ids.length} project{member.project_ids.length === 1 ? "" : "s"}</span>
              <span>
                {`last update ${formatRelativeTime(member.last_update_at, clock)}`}
              </span>
            </p>
          </div>
        </div>
      </header>

      <InnerTabNav
        activeId={view}
        ariaLabel="Member sections"
        items={tabItems}
      />

      {isActivityView ? (
        <section className="mt-7">
          <ActivityUpdateList
            emptyMessage="No member updates yet."
            errorMessage={activityError}
            isLoading={activityQuery.isLoading}
            loadingMessage="Loading member activity..."
            updates={activityQuery.data?.updates}
          />
        </section>
      ) : (
        <section className="mt-7 grid gap-6">
          <section className={cx(accentSurfaceClass, "grid gap-4 p-[18px]")}>
            <div className="flex items-center justify-between gap-3">
              <span className={sectionLabelClass}>Member summary</span>
              <time
                className="font-mono text-[0.72rem] text-ink-muted"
                dateTime={member.last_update_at}
              >
                {formatRelativeTime(member.last_update_at, clock)}
              </time>
            </div>
            {hasBluf ? (
              <MarkdownBlock markdown={member.bluf_markdown} />
            ) : (
              <p className={messageClass}>No summary has been generated yet.</p>
            )}
          </section>

          <section className={cx(subduedSurfaceClass, "grid gap-4 p-[18px]")}>
            <div className="flex items-center justify-between gap-3">
              <span className={sectionLabelClass}>Projects</span>
              <span className="font-mono text-[11px] uppercase text-ink-muted">
                {memberProjects.length} active
              </span>
            </div>

            {memberProjects.length > 0 ? (
              <div className="grid gap-3">
                {memberProjects.map((project) => (
                  <MemberProjectRow
                    key={project.project_id}
                    member={member}
                    project={project}
                  />
                ))}
              </div>
            ) : (
              <p className={messageClass}>
                This teammate isn't attached to any active projects right now.
              </p>
            )}
          </section>
        </section>
      )}
    </main>
  );
}

function MemberProjectRow({
  member,
  project,
}: {
  member: MemberSnapshot;
  project: ProjectListEntry;
}) {
  return (
    <Link
      className="block border border-border bg-[rgba(6,9,15,0.74)] p-[18px] no-underline transition duration-150 hover:-translate-y-px hover:border-border-strong"
      to={`/p/${project.project_id}`}
    >
      <div className="flex flex-col gap-2 text-ink sm:flex-row sm:items-center sm:justify-between">
        <strong>{project.name}</strong>
        <span className="font-mono text-[0.78rem] text-ink-muted">
          {project.project_id}
        </span>
      </div>
      <p className="mt-2.5 flex flex-wrap gap-2.5 font-mono text-[0.76rem] text-ink-dim">
        <span>
          {project.member_count} member{project.member_count === 1 ? "" : "s"}
        </span>
        <span>{readBlufPreview(project.bluf_markdown, member.member_name)}</span>
      </p>
    </Link>
  );
}

function readBlufPreview(markdown: string, _memberName: string) {
  const preview = markdown
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/[`*_>#-]/g, " ")
    .replace(/\s+/g, " ")
    .trim();

  if (!preview) {
    return "No project summary yet.";
  }

  return preview.length > 180 ? `${preview.slice(0, 180).trimEnd()}…` : preview;
}
