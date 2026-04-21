import type {
  OrganizationAgentContextExportFile,
  OrganizationAgentContextExportResponse,
  OrganizationWorkflowDocument,
  OrganizationWorkflowDocumentsResponse,
} from "./types";

interface RenderOrganizationAgentContextOptions {
  organizationSlug: string;
  memories: OrganizationWorkflowDocumentsResponse;
  skills: OrganizationWorkflowDocumentsResponse;
}

export function renderOrganizationAgentContextExport({
  organizationSlug,
  memories,
  skills,
}: RenderOrganizationAgentContextOptions): OrganizationAgentContextExportResponse {
  return {
    files: [
      renderWorkflowFile(
        organizationSlug,
        "memories",
        "Supermanager Organization Memories",
        "Durable organization context exported from Supermanager memory workflows. Treat this as stable imported context, not live system state.",
        memories.documents,
      ),
      renderWorkflowFile(
        organizationSlug,
        "skills",
        "Supermanager Organization Skills",
        "Reusable procedures exported from Supermanager skills workflows. Keep organization scoping explicit when applying any of these instructions.",
        skills.documents,
      ),
    ],
  };
}

function renderWorkflowFile(
  organizationSlug: string,
  fileName: string,
  title: string,
  description: string,
  documents: OrganizationWorkflowDocument[],
): OrganizationAgentContextExportFile {
  const sections =
    documents.length === 0
      ? ["No files are available yet for this workflow."]
      : orderDocuments(documents).map((document) =>
          [
            `## ${document.path}`,
            "",
            document.content.trim() || "_Empty file._",
          ].join("\n"),
        );

  return {
    path: `${fileName}.md`,
    content: [
      `# ${title}`,
      "",
      `- organization: ${organizationSlug}`,
      `- source: supermanager`,
      "",
      description,
      "",
      ...sections.flatMap((section, index) =>
        index === sections.length - 1 ? [section] : [section, "", "---", ""],
      ),
      "",
    ].join("\n"),
    updated_at: latestUpdatedAt(documents),
  };
}

function orderDocuments(documents: OrganizationWorkflowDocument[]) {
  return [...documents].sort((left, right) => {
    return documentSortKey(left.path).localeCompare(
      documentSortKey(right.path),
    );
  });
}

function documentSortKey(path: string) {
  if (path === "memory_summary.md") {
    return `00:${path}`;
  }
  if (path === "MEMORY.md") {
    return `01:${path}`;
  }
  return `10:${path}`;
}

function latestUpdatedAt(documents: OrganizationWorkflowDocument[]) {
  if (documents.length === 0) {
    return null;
  }

  return documents.reduce<string | null>((latest, document) => {
    if (latest == null || document.updated_at > latest) {
      return document.updated_at;
    }
    return latest;
  }, null);
}
