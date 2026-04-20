import { Link } from "react-router-dom";
import { useEffect, useState } from "react";
import { formatCount } from "../../lib/format-count";
import { displayEmployeeName } from "../../lib/display-employee-name";
import { formatRelativeTime } from "../../lib/format-relative-time";
import type {
  EmployeeSnapshot,
  OrganizationSnapshot,
  ProjectBlufSnapshot,
  ProjectListEntry,
  SummaryStatus,
  OrganizationMembership,
} from "../../api";
import {
  cx,
  errorMessageClass,
  messageClass,
  pillBaseClass,
  sectionLabelClass,
  subduedSurfaceClass,
} from "../../ui";
import { MarkdownBlock } from "../markdown-block";
import { OrgWideBlufCard } from "./org-wide-bluf-card";

interface OrganizationInsightsPanelProps {
  activeOrganization: OrganizationMembership | null;
  error: string | null;
  isLoading: boolean;
  organizationSummary: OrganizationSnapshot | null;
  projects: ProjectListEntry[];
  summaryStatus: SummaryStatus;
}

export function OrganizationInsightsPanel({
  activeOrganization,
  error,
  isLoading,
  organizationSummary,
  projects,
  summaryStatus,
}: OrganizationInsightsPanelProps) {
  const [clock, setClock] = useState(() => Date.now());
  const employees = organizationSummary?.employees ?? [];
  const projectBlufs = organizationSummary?.projects ?? [];
  const projectNames = new Map(projects.map((project) => [project.project_id, project.name]));
  const projectMetadata = new Map(projects.map((project) => [project.project_id, project]));

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
        <p className={messageClass}>Loading org insights...</p>
      ) : !activeOrganization ? (
        <p className={errorMessageClass}>Failed to load your workspace.</p>
      ) : (
        <div className="grid gap-6">
          <OrgWideBlufCard
            organizationSummary={organizationSummary}
            showStatusMeta
            summaryStatus={summaryStatus}
          />

          <div className="grid gap-6 xl:grid-cols-[minmax(0,1.08fr)_minmax(0,0.92fr)]">
            <section className={cx(subduedSurfaceClass, "p-[18px]")}>
              <div className="mb-[18px] flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
                <span className={sectionLabelClass}>Employees</span>
                <span className={`${pillBaseClass} border-border text-ink-dim`}>
                  {formatCount(employees.length, "summary", "summaries")}
                </span>
              </div>

              {employees.length > 0 ? (
                <div className="grid gap-4">
                  {employees.map((employee) => (
                    <EmployeeBlufCard
                      clock={clock}
                      employee={employee}
                      key={employeeCardKey(employee)}
                      projectNames={projectNames}
                    />
                  ))}
                </div>
              ) : (
                <p className={messageClass}>No employee summaries yet.</p>
              )}
            </section>

            <section className={cx(subduedSurfaceClass, "p-[18px]")}>
              <div className="mb-[18px] flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
                <span className={sectionLabelClass}>Projects</span>
                <span className={`${pillBaseClass} border-border text-ink-dim`}>
                  {formatCount(projectBlufs.length, "summary", "summaries")}
                </span>
              </div>

              {projectBlufs.length > 0 ? (
                <div className="grid gap-4">
                  {projectBlufs.map((projectBluf) => (
                    <ProjectBlufCard
                      clock={clock}
                      key={projectBluf.project_id}
                      projectBluf={projectBluf}
                      projectMetadata={projectMetadata.get(projectBluf.project_id)}
                    />
                  ))}
                </div>
              ) : (
                <p className={messageClass}>No project summaries yet.</p>
              )}
            </section>
          </div>
        </div>
      )}
    </section>
  );
}

function EmployeeBlufCard({
  clock,
  employee,
  projectNames,
}: {
  clock: number;
  employee: EmployeeSnapshot;
  projectNames: Map<string, string>;
}) {
  return (
    <article className="border border-border bg-[linear-gradient(180deg,rgba(16,23,34,0.82),rgba(8,12,19,0.94))] p-[18px]">
      <div className="mb-3.5 flex flex-col gap-3">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-baseline sm:justify-between">
          <h3 className="m-0 text-[1.05rem] font-semibold text-ink">
            {displayEmployeeName(employee.employee_name)}
          </h3>
          <time
            className="font-mono text-[0.72rem] text-ink-muted"
            dateTime={employee.last_update_at}
          >
            {formatRelativeTime(employee.last_update_at, clock)}
          </time>
        </div>
        <div className="flex flex-wrap gap-2">
          {employee.project_ids.map((projectId) => (
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
      <MarkdownBlock markdown={employee.bluf_markdown} />
    </article>
  );
}

function ProjectBlufCard({
  clock,
  projectBluf,
  projectMetadata,
}: {
  clock: number;
  projectBluf: ProjectBlufSnapshot;
  projectMetadata?: ProjectListEntry;
}) {
  return (
    <article className="border border-border bg-[linear-gradient(180deg,rgba(16,23,34,0.82),rgba(8,12,19,0.94))] p-[18px]">
      <div className="mb-3.5 flex flex-col gap-3">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-baseline sm:justify-between">
          <div className="min-w-0">
            <Link
              className="text-[1.05rem] font-semibold text-ink no-underline transition hover:text-accent"
              to={`/p/${projectBluf.project_id}`}
            >
              {projectMetadata?.name ?? projectBluf.project_id}
            </Link>
            {projectMetadata?.name ? (
              <p className="mt-2 font-mono text-[0.72rem] text-ink-muted">
                {projectBluf.project_id}
              </p>
            ) : null}
          </div>
          <time
            className="font-mono text-[0.72rem] text-ink-muted"
            dateTime={projectBluf.last_update_at}
          >
            {formatRelativeTime(projectBluf.last_update_at, clock)}
          </time>
        </div>

        {projectMetadata ? (
          <p className="font-mono text-[0.76rem] text-ink-dim">
            {projectMetadata.employee_count} employee
            {projectMetadata.employee_count === 1 ? "" : "s"}
          </p>
        ) : null}
      </div>

      <MarkdownBlock markdown={projectBluf.bluf_markdown} />
    </article>
  );
}

function employeeCardKey(employee: {
  employee_name: string;
  employee_user_id: string;
}) {
  return employee.employee_user_id;
}
