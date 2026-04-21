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
import type { OrganizationDocumentsView } from "../../queries/organization-documents";

interface OrganizationDocumentsPanelProps {
  activeOrganization: OrganizationMembership | null;
  documentsResponse: OrganizationWorkflowDocumentsResponse | null;
  error: string | null;
  isLoading: boolean;
  view: OrganizationDocumentsView;
}

export function OrganizationDocumentsPanel({
  activeOrganization,
  documentsResponse,
  error,
  isLoading,
  view,
}: OrganizationDocumentsPanelProps) {
  const [clock, setClock] = useState(() => Date.now());
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const documents = documentsResponse?.documents ?? [];
  const defaultPath = useMemo(
    () => preferredDocumentPath(view, documents),
    [documents, view],
  );
  const selectedDocument =
    documents.find((document) => document.path === selectedPath) ?? null;
  const title = view === "memories" ? "Organization memories" : "Organization skills";
  const description =
    view === "memories"
      ? "Durable notes, recurring decisions, and reusable context collected across the organization."
      : "Reusable procedures the agent has inferred from repeated work across the organization.";
  const emptyMessage =
    view === "memories"
      ? "No organization memories have been written yet. The daily workflow will populate them once durable patterns emerge."
      : "No organization skills have been written yet. Repeated procedures will show up here once the workflow promotes them.";

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClock(Date.now());
    }, 30_000);

    return () => {
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (!defaultPath) {
      setSelectedPath(null);
      return;
    }

    if (!selectedPath || !documents.some((document) => document.path === selectedPath)) {
      setSelectedPath(defaultPath);
    }
  }, [defaultPath, documents, selectedPath]);

  return (
    <section className="mt-7 grid gap-6">
      {error && <p className={errorMessageClass}>{error}</p>}

      {isLoading ? (
        <p className={messageClass}>
          Loading {view === "memories" ? "organization memories" : "organization skills"}...
        </p>
      ) : !activeOrganization ? (
        <p className={errorMessageClass}>Failed to load your workspace.</p>
      ) : documents.length === 0 ? (
        <section className={cx(subduedSurfaceClass, "grid gap-4 p-[18px]")}>
          <span className={sectionLabelClass}>{title}</span>
          <h2 className="m-0 text-3xl font-semibold leading-tight text-ink sm:text-4xl">
            {activeOrganization.organization_name}
          </h2>
          <p className={messageClass}>{description}</p>
          <p className={messageClass}>{emptyMessage}</p>
        </section>
      ) : (
        <div className="grid gap-6 xl:grid-cols-[minmax(0,280px)_minmax(0,1fr)]">
          <section className={cx(subduedSurfaceClass, "grid gap-4 p-[18px]")}>
            <div className="flex flex-col gap-3">
              <div className="flex items-center justify-between gap-3">
                <span className={sectionLabelClass}>{title}</span>
                <span className={`${pillBaseClass} border-border text-ink-dim`}>
                  {documents.length} file{documents.length === 1 ? "" : "s"}
                </span>
              </div>
              <p className={messageClass}>{description}</p>
            </div>

            <div className="grid gap-3">
              {documents.map((document) => {
                const isActive = document.path === selectedDocument?.path;
                return (
                  <button
                    className={cx(
                      "grid gap-2 border px-3 py-3 text-left transition duration-150",
                      isActive
                        ? "border-accent/40 bg-white/6"
                        : "border-border bg-panel hover:border-border-strong hover:bg-white/[0.03]",
                    )}
                    key={document.path}
                    type="button"
                    onClick={() => setSelectedPath(document.path)}
                  >
                    <span className="text-sm font-semibold text-ink">
                      {documentLabel(document.path)}
                    </span>
                    <span className="font-mono text-[0.72rem] text-ink-muted">
                      {documentsResponse?.path_root}/{document.path}
                    </span>
                    <time
                      className="font-mono text-[0.72rem] text-ink-dim"
                      dateTime={document.updated_at}
                    >
                      {formatRelativeTime(document.updated_at, clock)}
                    </time>
                  </button>
                );
              })}
            </div>
          </section>

          <section className={cx(subduedSurfaceClass, "grid gap-5 p-[18px]")}>
            {selectedDocument ? (
              <>
                <div className="grid gap-3 border-b border-border pb-5">
                  <span className={sectionLabelClass}>Selected file</span>
                  <h2 className="m-0 break-words text-3xl font-semibold leading-tight text-ink sm:text-4xl">
                    {documentLabel(selectedDocument.path)}
                  </h2>
                  <p className={projectMetaClass}>
                    <span>{documentsResponse?.path_root}/{selectedDocument.path}</span>
                    <span>{`updated ${formatRelativeTime(selectedDocument.updated_at, clock)}`}</span>
                  </p>
                </div>

                <DocumentContent document={selectedDocument} />
              </>
            ) : (
              <p className={messageClass}>Select a file to read it.</p>
            )}
          </section>
        </div>
      )}
    </section>
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

function documentLabel(path: string) {
  if (path.endsWith("/SKILL.md")) {
    return path.slice(0, -"/SKILL.md".length) || path;
  }

  return path;
}

function preferredDocumentPath(
  view: OrganizationDocumentsView,
  documents: OrganizationWorkflowDocument[],
) {
  if (documents.length === 0) {
    return null;
  }

  if (view === "memories") {
    return (
      documents.find((document) => document.path === "memory_summary.md")?.path ??
      documents.find((document) => document.path === "MEMORY.md")?.path ??
      documents[0]!.path
    );
  }

  return documents[0]!.path;
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
