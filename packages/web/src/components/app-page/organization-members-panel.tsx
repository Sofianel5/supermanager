import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import type {
  MemberSnapshot,
  OrganizationMembership,
  ProjectListEntry,
} from "../../api";
import { displayMemberName } from "../../lib/display-member-name";
import { formatRelativeTime } from "../../lib/format-relative-time";
import { buildMemberHref } from "../../lib/organization";
import {
  cx,
  errorMessageClass,
  messageClass,
  subduedSurfaceClass,
} from "../../ui";
import { MarkdownBlock } from "../markdown-block";
import { MemberAvatar } from "../member-avatar";

interface OrganizationMembersPanelProps {
  activeOrganization: OrganizationMembership | null;
  error: string | null;
  isLoading: boolean;
  members: MemberSnapshot[];
  organizationSlug: string | null;
  projects: ProjectListEntry[];
}

export function OrganizationMembersPanel({
  activeOrganization,
  error,
  isLoading,
  members,
  organizationSlug,
  projects,
}: OrganizationMembersPanelProps) {
  const [clock, setClock] = useState(() => Date.now());
  const projectNames = new Map(
    projects.map((project) => [project.project_id, project.name]),
  );

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClock(Date.now());
    }, 30_000);

    return () => {
      window.clearInterval(timer);
    };
  }, []);

  return (
    <section className="mt-7 grid gap-6">
      {error && <p className={errorMessageClass}>{error}</p>}

      {isLoading ? (
        <p className={messageClass}>Loading members...</p>
      ) : !activeOrganization ? (
        <p className={errorMessageClass}>Failed to load your workspace.</p>
      ) : members.length === 0 ? (
        <section className={cx(subduedSurfaceClass, "grid gap-3 p-[18px]")}>
          <p className={messageClass}>
            No member summaries yet. They appear as the workflow processes activity
            from each teammate.
          </p>
        </section>
      ) : (
        <div className="grid gap-4">
          {members.map((member) => (
            <MemberCard
              clock={clock}
              key={member.member_user_id}
              member={member}
              organizationSlug={organizationSlug}
              projectNames={projectNames}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function MemberCard({
  clock,
  member,
  organizationSlug,
  projectNames,
}: {
  clock: number;
  member: MemberSnapshot;
  organizationSlug: string | null;
  projectNames: Map<string, string>;
}) {
  const memberName = displayMemberName(member.member_name);

  return (
    <article className="border border-border bg-[linear-gradient(180deg,rgba(16,23,34,0.82),rgba(8,12,19,0.94))] p-[18px]">
      <div className="mb-3.5 flex items-start gap-4">
        <MemberAvatar name={memberName} />
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          <div className="flex flex-col gap-2 sm:flex-row sm:items-baseline sm:justify-between">
            <Link
              className="text-[1.05rem] font-semibold text-ink no-underline transition hover:text-accent"
              to={buildMemberHref(member.member_user_id, organizationSlug)}
            >
              {memberName}
            </Link>
            <time
              className="font-mono text-[0.72rem] text-ink-muted"
              dateTime={member.last_update_at}
            >
              {formatRelativeTime(member.last_update_at, clock)}
            </time>
          </div>
          <div className="flex flex-wrap gap-2">
            {member.project_ids.map((projectId) => (
              <Link
                className="inline-flex min-h-[28px] items-center border border-border px-2.5 font-mono text-[11px] uppercase text-ink-dim no-underline transition duration-150 hover:border-border-strong hover:text-ink"
                key={projectId}
                to={`/p/${projectId}`}
              >
                {projectNames.get(projectId) ?? projectId}
              </Link>
            ))}
          </div>
        </div>
      </div>
      <MarkdownBlock markdown={member.bluf_markdown} />
    </article>
  );
}
