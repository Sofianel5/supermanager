import { useState } from "react";
import { Link } from "react-router-dom";
import type {
  ActivityUpdate,
  OrganizationMembership,
  OrganizationSnapshot,
  OrganizationWorkflowDocumentsResponse,
  ProjectListEntry,
  SummaryStatus,
} from "../api";
import { ActivityUpdateList } from "../components/activity-update-list";
import { OrganizationKnowledgePanel } from "../components/app-page/organization-knowledge-panel";
import { OrganizationMembersPanel } from "../components/app-page/organization-members-panel";
import { InnerTabNav, type InnerTabItem } from "../components/inner-tab-nav";
import { OrgWideBlufCard } from "../components/app-page/org-wide-bluf-card";
import { WorkspaceHeader } from "../components/app-page/workspace-header";
import { WorkspacePanel } from "../components/app-page/workspace-panel";
import { useCopyHandler } from "../utils";
import {
  copyLabelClass,
  copySheetClass,
  cx,
  pageShellClass,
  primaryButtonClass,
  secondaryButtonClass,
  sectionLabelClass,
} from "../ui";

const INSTALL_COMMAND = "curl -fsSL https://supermanager.dev/install.sh | sh";

const landingBenefits = [
  {
    body: "Connected repos stream Claude Code and Codex activity into one place, so the team can catch up without another status loop.",
    title: "Live project visibility",
  },
  {
    body: "See who is driving each project, where work is moving, and where someone may need backup, without interrupting the team for another round of check-ins.",
    title: "Stay in sync with the team",
  },
  {
    body: "As your team works, Supermanager automatically turns repeated patterns into shared memories and reusable skills, so good workflows spread instead of staying stuck in one terminal.",
    title: "Automatic shared memory and skills",
  },
] as const;

type PreviewTabId = "projects" | "members" | "activity" | "knowledge";

const previewOrganization: OrganizationMembership = {
  member_count: 8,
  organization_id: "org_preview",
  organization_name: "Daily Planet",
  organization_slug: "daily-planet",
  role: "owner",
};

const previewSummary: OrganizationSnapshot = {
  bluf_markdown: [
    "- Metropolis launch is greenlit, pending one last pass on the emergency-banner copy after Legal objected to `faster than a speeding deploy`.",
    "- Fortress cache cleanup landed, response time is down, and no one has blamed alien crystals for outages all week.",
    "- Org memory already links the last LexCorp incident, the containment checklist, and the standing rule against accepting free meteor samples.",
  ].join("\n"),
  members: [
    {
      bluf_markdown:
        "Keeps the Metropolis launch unblocked, writes sharper rollout notes than anyone else in the newsroom, and refuses to let heroics replace process.",
      last_update_at: "2026-04-20T17:24:00.000Z",
      member_name: "Lois Lane",
      member_user_id: "user_lois",
      project_ids: ["metropolis-web"],
    },
    {
      bluf_markdown:
        "Closed the Fortress cache gap, verified the core path in prod, and somehow filed the cleanest incident notes in the building before lunch.",
      last_update_at: "2026-04-20T17:12:00.000Z",
      member_name: "Clark Kent",
      member_user_id: "user_clark",
      project_ids: ["fortress-core"],
    },
    {
      bluf_markdown:
        "Turned the field recap around fast and linked it to the right org memory before Perry could ask where the photos were.",
      last_update_at: "2026-04-20T16:58:00.000Z",
      member_name: "Jimmy Olsen",
      member_user_id: "user_jimmy",
      project_ids: ["signal-watch"],
    },
  ],
  projects: [],
};

const previewActivityUpdates: ActivityUpdate[] = [
  {
    created_at: "2026-04-20T17:31:00.000Z",
    statement_text:
      "Lois finalized the Metropolis launch notes and removed three references to `faster than a speeding deploy` after Legal pushed back.",
  },
  {
    created_at: "2026-04-20T17:16:00.000Z",
    statement_text:
      "Clark closed the Fortress cache issue, confirmed the gateway recovery, and politely documented that the crystals were not the root cause.",
  },
  {
    created_at: "2026-04-20T16:59:00.000Z",
    statement_text:
      "Jimmy linked the Signal Watch incident to the LexCorp memory entry because `giant robot downtown` was still not enough context for future triage.",
  },
];

const previewMemoriesResponse: OrganizationWorkflowDocumentsResponse = {
  documents: [
    {
      content: [
        "---",
        "owner: lois-lane",
        "severity: high",
        "---",
        "# LexCorp Kryptonite Leak",
        "",
        "- Do not carry samples into the newsroom.",
        "- Facilities gets paged before Superman notices.",
        "- Use the containment checklist before anyone says `it is probably fine`.",
      ].join("\n"),
      path: "incidents/lexcorp-kryptonite-leak.md",
      updated_at: "2026-04-20T17:22:00.000Z",
    },
    {
      content: [
        "---",
        "owner: jimmy-olsen",
        "kind: launch-note",
        "---",
        "# Metropolis Launch Day",
        "",
        "- Banner fallback lives in the newsroom CDN.",
        "- Transit alerts stay on until the sky is quiet.",
        "- Perry wants the all-clear before the victory headline goes live.",
      ].join("\n"),
      path: "operations/metropolis-launch-day.md",
      updated_at: "2026-04-20T16:40:00.000Z",
    },
  ],
  path_root: "organization/memories",
};

const previewSkillsResponse: OrganizationWorkflowDocumentsResponse = {
  documents: [
    {
      content: [
        "---",
        "owner: clark-kent",
        "trigger: critical-platform-incident",
        "---",
        "# Fortress Cache Recovery",
        "",
        "1. Verify the cache key rollout before touching the gateway.",
        "2. Compare the current incident against the last Fortress outage.",
        "3. Escalate only after ruling out LexCorp sabotage and ordinary operator error.",
      ].join("\n"),
      path: "fortress-cache-recovery/SKILL.md",
      updated_at: "2026-04-20T17:18:00.000Z",
    },
    {
      content: [
        "---",
        "owner: lois-lane",
        "trigger: citywide-alert",
        "---",
        "# Metropolis Alert Copy",
        "",
        "1. State what happened in plain language.",
        "2. Do not imply Superman has already fixed it unless confirmed.",
        "3. Keep the fallback banner sharper than the mayor's press office can manage.",
      ].join("\n"),
      path: "metropolis-alert-copy/SKILL.md",
      updated_at: "2026-04-20T16:48:00.000Z",
    },
  ],
  path_root: "organization/skills",
};

const previewProjects: ProjectListEntry[] = [
  {
    bluf_markdown:
      "The Metropolis launch is staged, the commuter alerts are behaving, and the fallback plan no longer assumes Superman is available for traffic control.",
    created_at: "2026-04-16T15:12:00.000Z",
    member_count: 3,
    name: "Metropolis web",
    organization_slug: previewOrganization.organization_slug,
    project_id: "metropolis-web",
  },
  {
    bluf_markdown:
      "Fortress cache invalidation landed, the core gateway is running faster, and the crystals remain innocent until proven guilty.",
    created_at: "2026-04-12T09:45:00.000Z",
    member_count: 2,
    name: "Fortress core",
    organization_slug: previewOrganization.organization_slug,
    project_id: "fortress-core",
  },
  {
    bluf_markdown:
      "Signal Watch now carries incident context into the next session, which is helping a lot because 'giant robot downtown' is still not a sufficient bug report.",
    created_at: "2026-04-09T11:30:00.000Z",
    member_count: 3,
    name: "Signal Watch",
    organization_slug: previewOrganization.organization_slug,
    project_id: "signal-watch",
  },
];

const previewTabItems: Array<InnerTabItem<PreviewTabId>> = [
  {
    count: previewProjects.length,
    id: "projects",
    label: "Projects",
    to: "/app",
  },
  {
    count: previewSummary.members.length,
    id: "members",
    label: "Members",
    to: "/app/members",
  },
  {
    id: "activity",
    label: "Activity",
    to: "/app/activity",
  },
  {
    count:
      previewMemoriesResponse.documents.length +
      previewSkillsResponse.documents.length,
    id: "knowledge",
    label: "Knowledge",
    to: "/app/knowledge",
  },
];

const previewSummaryStatus: SummaryStatus = "ready";

export function LandingPage() {
  const { copiedValue, copy } = useCopyHandler();

  return (
    <main className="relative overflow-hidden">
      <Hero />

      <section className={cx(pageShellClass, "pt-14")}>
        <div className="border-t border-border pt-8">
          <div className={sectionLabelClass}>Why teams use it</div>
          <div className="mt-6 grid gap-8 lg:grid-cols-3 lg:gap-0">
            {landingBenefits.map((benefit, index) => (
              <article
                className={cx(
                  "grid content-start gap-4 border-border lg:pr-8",
                  index > 0 ? "lg:border-l lg:pl-8" : "",
                )}
                key={benefit.title}
              >
                <h2 className="m-0 text-[1.6rem] font-semibold leading-tight tracking-[-0.03em] text-ink">
                  {benefit.title}
                </h2>
                <p className="m-0 max-w-[30rem] text-[1rem] leading-7 text-ink-dim">
                  {benefit.body}
                </p>
              </article>
            ))}
          </div>
        </div>
      </section>

      <section className={cx(pageShellClass, "pb-16 pt-18")}>
        <div className="grid gap-7 border-t border-border pt-8 lg:grid-cols-[minmax(0,0.92fr)_minmax(320px,0.78fr)] lg:items-end">
          <div className="grid gap-4">
            <div className={sectionLabelClass}>Get started</div>
            <h2 className="m-0 max-w-[12ch] text-[clamp(2.5rem,6vw,4.4rem)] font-semibold leading-[0.94] tracking-[-0.05em] text-ink">
              Try the research preview today.
            </h2>
            <p className="m-0 max-w-[40rem] text-[1.02rem] leading-8 text-ink-dim">
              Supermanager is the shared collaboration &amp; context layer
              around the AI tools your team already uses. Sign in from the
              browser, connect a project from the CLI, and the workspace starts
              filling itself in.
            </p>
          </div>

          <div className="grid gap-4">
            <button
              className={copySheetClass}
              type="button"
              onClick={() => void copy("install", INSTALL_COMMAND)}
            >
              <span className={copyLabelClass}>
                {copiedValue === "install" ? "copied" : "click to copy"}
              </span>
              <code className="mt-2.5 block break-words font-mono text-[13px] leading-7 text-[#f4bf63]">
                {INSTALL_COMMAND}
              </code>
            </button>

            <div className="flex flex-wrap gap-3">
              <Link className={primaryButtonClass} to="/login">
                Continue to login
              </Link>
              <Link className={secondaryButtonClass} to="/docs">
                Read docs
              </Link>
            </div>
          </div>
        </div>
      </section>
    </main>
  );
}

function Hero() {
  return (
    <section className="relative overflow-hidden border-b border-border">
      <div className="pointer-events-none absolute inset-0">
        <div className="absolute inset-x-0 top-[-24rem] h-[34rem] bg-[radial-gradient(circle_at_center,rgba(245,158,11,0.16),transparent_62%)]" />
        <div className="absolute bottom-[-18rem] right-[-8rem] h-[28rem] w-[34rem] bg-[radial-gradient(circle_at_center,rgba(41,121,255,0.18),transparent_64%)]" />
      </div>

      <div className="relative mx-auto flex min-h-screen w-full max-w-[1280px] flex-col px-5 pb-14 pt-7 max-[900px]:min-h-[auto] max-[900px]:px-[14px] max-[640px]:px-[10px]">
        <header className="flex items-center justify-end gap-3">
          <nav className="flex flex-wrap items-center justify-end gap-3">
            <Link className={secondaryButtonClass} to="/docs">
              Docs
            </Link>
            <Link className={primaryButtonClass} to="/login">
              Login
            </Link>
          </nav>
        </header>

        <div className="grid flex-1 content-center gap-8 pb-4 pt-10 max-[900px]:pt-12">
          <div className="mx-auto max-w-[900px] text-center">
            <div className="animate-[rise-in_700ms_ease-out_both]">
              <p className="m-0 text-[clamp(2.85rem,11vw,8.8rem)] font-semibold leading-[0.88] tracking-[-0.09em] text-ink">
                supermanager
              </p>
              <h1 className="mx-auto mt-5 max-w-[14ch] text-[clamp(2.15rem,5vw,4.3rem)] font-semibold leading-[0.98] tracking-[-0.055em] text-ink">
                The shared context layer for your team.
              </h1>
              <p className="mx-auto mt-5 max-w-[46rem] text-[1.08rem] leading-8 text-ink-dim">
                Supermanager turns Claude Code and Codex activity into a shared
                workspace with live activity feeds, summaries, and useful memory
                for your team & agents.
              </p>
            </div>

            <div className="mt-8 flex flex-wrap items-center justify-center gap-3 animate-[rise-in_700ms_ease-out_120ms_both]">
              <Link className={primaryButtonClass} to="/login">
                Continue to login
              </Link>
              <Link className={secondaryButtonClass} to="/docs">
                See docs
              </Link>
            </div>
          </div>

          <div className="animate-[rise-in_850ms_ease-out_320ms_both]">
            <LandingPreview />
          </div>
        </div>
      </div>
    </section>
  );
}

function LandingPreview() {
  const [activeTab, setActiveTab] = useState<PreviewTabId>("projects");

  return (
    <div className="mx-auto w-full max-w-[1160px] animate-[hero-float_12s_ease-in-out_infinite]">
      <div className="overflow-hidden rounded-[28px] border border-border-strong shadow-[0_40px_120px_rgba(2,6,23,0.58)]">
        <div className="relative bg-[#06080e]">
          <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(980px_460px_at_50%_-80px,rgba(245,158,11,0.09),transparent_72%),radial-gradient(720px_420px_at_100%_100%,rgba(24,118,210,0.1),transparent_68%),linear-gradient(180deg,#090d15_0%,#06080e_100%)]" />
          <div className="pointer-events-none absolute inset-0 bg-[linear-gradient(rgba(255,255,255,0.04)_1px,transparent_1px),linear-gradient(90deg,rgba(255,255,255,0.04)_1px,transparent_1px)] bg-[size:72px_72px] [mask-image:radial-gradient(circle_at_center,black,transparent_78%)]" />

          <div className="relative select-none">
            <main className="mx-auto w-full max-w-[1180px] px-5 pb-[56px] max-[900px]:px-[14px] max-[640px]:px-[10px]">
              <div className="pointer-events-none">
                <WorkspaceHeader
                  activeOrganizationName={previewOrganization.organization_name}
                  activeOrganizationSlug={previewOrganization.organization_slug}
                  isSigningOut={false}
                  userEmail="clark@dailyplanet.press"
                  onInviteTeammate={noop}
                  onOpenDocs={noop}
                  onSignOut={noop}
                />

                <div className="mt-7">
                  <OrgWideBlufCard
                    organizationSummary={previewSummary}
                    showStatusMeta
                    summaryStatus={previewSummaryStatus}
                  />
                </div>
              </div>

              <div className="pointer-events-auto">
                <InnerTabNav
                  activeId={activeTab}
                  ariaLabel="Preview sections"
                  items={previewTabItems}
                  onSelect={setActiveTab}
                />
              </div>

              <div className="pointer-events-none">
                {activeTab === "projects" ? (
                  <div className="mt-7">
                    <WorkspacePanel
                      activeOrganization={previewOrganization}
                      error={null}
                      isCreatingProject={false}
                      isLoading={false}
                      projects={previewProjects}
                      onCreateProject={noop}
                    />
                  </div>
                ) : activeTab === "members" ? (
                  <OrganizationMembersPanel
                    activeOrganization={previewOrganization}
                    error={null}
                    isLoading={false}
                    members={previewSummary.members}
                    organizationSlug={previewOrganization.organization_slug}
                  />
                ) : activeTab === "activity" ? (
                  <section className="mt-7">
                    <ActivityUpdateList
                      emptyMessage="No organization updates yet."
                      isLoading={false}
                      updates={previewActivityUpdates}
                    />
                  </section>
                ) : (
                  <OrganizationKnowledgePanel
                    activeOrganization={previewOrganization}
                    error={null}
                    isLoading={false}
                    memoriesResponse={previewMemoriesResponse}
                    skillsResponse={previewSkillsResponse}
                  />
                )}
              </div>
            </main>
          </div>
        </div>
      </div>
    </div>
  );
}

function noop() {}
