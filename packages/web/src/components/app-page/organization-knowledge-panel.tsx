import { useEffect, useMemo, useState } from "react";
import type {
  OrganizationMembership,
  OrganizationWorkflowDocument,
  OrganizationWorkflowDocumentsResponse,
} from "../../api";
import { formatRelativeTime } from "../../lib/format-relative-time";
import {
  cx,
  errorMessageClass,
  messageClass,
  pillBaseClass,
  projectMetaClass,
  sectionLabelClass,
  subduedSurfaceClass,
} from "../../ui";
import { MarkdownBlock } from "../markdown-block";

type DocumentKind = "memory" | "skill";
type KnowledgeFilter = "all" | "memory" | "skill";

interface OrganizationKnowledgePanelProps {
  activeOrganization: OrganizationMembership | null;
  error: string | null;
  isLoading: boolean;
  memoriesResponse: OrganizationWorkflowDocumentsResponse | null;
  skillsResponse: OrganizationWorkflowDocumentsResponse | null;
}

interface KnowledgeEntry {
  kind: DocumentKind;
  pathRoot: string;
  document: OrganizationWorkflowDocument;
}

export function OrganizationKnowledgePanel({
  activeOrganization,
  error,
  isLoading,
  memoriesResponse,
  skillsResponse,
}: OrganizationKnowledgePanelProps) {
  const [clock, setClock] = useState(() => Date.now());
  const [filter, setFilter] = useState<KnowledgeFilter>("all");
  const [selectedKey, setSelectedKey] = useState<string | null>(null);

  const entries = useMemo(
    () => buildEntries(memoriesResponse, skillsResponse),
    [memoriesResponse, skillsResponse],
  );
  const filteredEntries = useMemo(
    () => (filter === "all" ? entries : entries.filter((entry) => entry.kind === filter)),
    [entries, filter],
  );
  const selectedEntry =
    filteredEntries.find((entry) => entryKey(entry) === selectedKey) ??
    filteredEntries[0] ??
    null;
  const memoryCount = entries.filter((entry) => entry.kind === "memory").length;
  const skillCount = entries.filter((entry) => entry.kind === "skill").length;

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClock(Date.now());
    }, 30_000);

    return () => {
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (!selectedEntry) {
      if (selectedKey !== null) {
        setSelectedKey(null);
      }
      return;
    }

    const currentKey = entryKey(selectedEntry);
    if (selectedKey !== currentKey) {
      setSelectedKey(currentKey);
    }
  }, [selectedEntry, selectedKey]);

  return (
    <section className="mt-7 grid gap-6">
      {error && <p className={errorMessageClass}>{error}</p>}

      {isLoading ? (
        <p className={messageClass}>Loading organization knowledge...</p>
      ) : !activeOrganization ? (
        <p className={errorMessageClass}>Failed to load your workspace.</p>
      ) : entries.length === 0 ? (
        <section className={cx(subduedSurfaceClass, "grid gap-3 p-[18px]")}>
          <span className={sectionLabelClass}>Organization knowledge</span>
          <p className={messageClass}>
            Durable notes and reusable procedures collected across the organization.
            They appear here once the workflow promotes them.
          </p>
        </section>
      ) : (
        <>
          <KnowledgeFilters
            activeFilter={filter}
            entryCount={entries.length}
            memoryCount={memoryCount}
            onChange={setFilter}
            skillCount={skillCount}
          />

          <div className="grid gap-6 xl:grid-cols-[minmax(0,320px)_minmax(0,1fr)]">
            <section className={cx(subduedSurfaceClass, "min-w-0 grid gap-3 p-[18px]")}>
              {filteredEntries.length > 0 ? (
                filteredEntries.map((entry) => (
                  <button
                    className={cx(
                      "min-w-0 grid gap-2 border px-3 py-3 text-left transition duration-150",
                      selectedEntry && entryKey(entry) === entryKey(selectedEntry)
                        ? "border-accent/40 bg-white/6"
                        : "border-border bg-panel hover:border-border-strong hover:bg-white/[0.03]",
                    )}
                    key={entryKey(entry)}
                    onClick={() => setSelectedKey(entryKey(entry))}
                    type="button"
                  >
                    <div className="min-w-0 flex items-center gap-2">
                      <KindChip kind={entry.kind} />
                      <span className="min-w-0 truncate text-sm font-semibold text-ink">
                        {documentLabel(entry)}
                      </span>
                    </div>
                    <span className="font-mono text-[0.72rem] text-ink-muted [overflow-wrap:anywhere]">
                      {entry.pathRoot}/{entry.document.path}
                    </span>
                    <time
                      className="font-mono text-[0.72rem] text-ink-dim"
                      dateTime={entry.document.updated_at}
                    >
                      {formatRelativeTime(entry.document.updated_at, clock)}
                    </time>
                  </button>
                ))
              ) : (
                <p className={messageClass}>No entries match this filter.</p>
              )}
            </section>

            <section className={cx(subduedSurfaceClass, "min-w-0 grid gap-5 p-[18px]")}>
              {selectedEntry ? (
                <>
                  <div className="grid gap-3 border-b border-border pb-5">
                    <div className="flex items-center gap-2">
                      <KindChip kind={selectedEntry.kind} />
                      <span className={sectionLabelClass}>Selected</span>
                    </div>
                    <h2 className="m-0 text-3xl font-semibold leading-tight text-ink [overflow-wrap:anywhere] sm:text-4xl">
                      {documentLabel(selectedEntry)}
                    </h2>
                    <p className={cx(projectMetaClass, "min-w-0")}>
                      <span className="[overflow-wrap:anywhere]">
                        {selectedEntry.pathRoot}/{selectedEntry.document.path}
                      </span>
                      <span>{`updated ${formatRelativeTime(
                        selectedEntry.document.updated_at,
                        clock,
                      )}`}</span>
                    </p>
                  </div>
                  <DocumentContent document={selectedEntry.document} />
                </>
              ) : (
                <p className={messageClass}>Select an entry to read it.</p>
              )}
            </section>
          </div>
        </>
      )}
    </section>
  );
}

function KnowledgeFilters({
  activeFilter,
  entryCount,
  memoryCount,
  onChange,
  skillCount,
}: {
  activeFilter: KnowledgeFilter;
  entryCount: number;
  memoryCount: number;
  onChange(filter: KnowledgeFilter): void;
  skillCount: number;
}) {
  const filters: Array<{ id: KnowledgeFilter; label: string; count: number }> = [
    { id: "all", label: "All", count: entryCount },
    { id: "memory", label: "Memories", count: memoryCount },
    { id: "skill", label: "Skills", count: skillCount },
  ];

  return (
    <div className="flex flex-wrap gap-2">
      {filters.map((filter) => {
        const isActive = filter.id === activeFilter;
        return (
          <button
            className={cx(
              pillBaseClass,
              "transition duration-150",
              isActive
                ? "border-accent/40 bg-white/6 text-accent"
                : "border-border text-ink-dim hover:border-border-strong hover:text-ink",
            )}
            key={filter.id}
            onClick={() => onChange(filter.id)}
            type="button"
          >
            {filter.label} · {filter.count}
          </button>
        );
      })}
    </div>
  );
}

function KindChip({ kind }: { kind: DocumentKind }) {
  const label = kind === "memory" ? "memory" : "skill";
  const tone =
    kind === "memory"
      ? "border-accent/30 text-accent"
      : "border-[rgba(127,183,255,0.35)] text-[#7FB7FF]";
  return (
      <span
        className={cx(
        "inline-flex min-h-[22px] shrink-0 items-center border px-2 font-mono text-[10px] uppercase tracking-[0.08em]",
        tone,
      )}
    >
      {label}
    </span>
  );
}

function DocumentContent({ document }: { document: OrganizationWorkflowDocument }) {
  const parsed = splitFrontmatter(document.content);

  return (
    <div className="grid gap-5">
      {parsed.frontmatter ? (
        <pre className="overflow-x-auto border border-border bg-panel px-4 py-3 font-mono text-[0.82rem] leading-6 text-ink-dim">
          {parsed.frontmatter}
        </pre>
      ) : null}

      {parsed.body.trim() ? (
        <MarkdownBlock markdown={parsed.body} />
      ) : (
        <p className={messageClass}>This file is empty.</p>
      )}
    </div>
  );
}

function buildEntries(
  memoriesResponse: OrganizationWorkflowDocumentsResponse | null,
  skillsResponse: OrganizationWorkflowDocumentsResponse | null,
): KnowledgeEntry[] {
  const entries: KnowledgeEntry[] = [];

  if (memoriesResponse) {
    for (const document of memoriesResponse.documents) {
      entries.push({
        document,
        kind: "memory",
        pathRoot: memoriesResponse.path_root,
      });
    }
  }

  if (skillsResponse) {
    for (const document of skillsResponse.documents) {
      entries.push({
        document,
        kind: "skill",
        pathRoot: skillsResponse.path_root,
      });
    }
  }

  entries.sort((a, b) =>
    compareTimestamps(b.document.updated_at, a.document.updated_at),
  );
  return entries;
}

function compareTimestamps(a: string, b: string) {
  const left = Date.parse(a);
  const right = Date.parse(b);
  if (Number.isNaN(left) || Number.isNaN(right)) {
    return 0;
  }
  return left - right;
}

function entryKey(entry: KnowledgeEntry) {
  return `${entry.kind}:${entry.document.path}`;
}

function documentLabel(entry: KnowledgeEntry) {
  const { path } = entry.document;
  if (entry.kind === "skill" && path.endsWith("/SKILL.md")) {
    return path.slice(0, -"/SKILL.md".length) || path;
  }
  return path;
}

function splitFrontmatter(content: string) {
  if (!content.startsWith("---\n")) {
    return { body: content, frontmatter: null };
  }

  const end = content.indexOf("\n---\n", 4);
  if (end === -1) {
    return { body: content, frontmatter: null };
  }

  return {
    frontmatter: content.slice(4, end).trim(),
    body: content.slice(end + 5),
  };
}
