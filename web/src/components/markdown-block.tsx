import { type ComponentPropsWithoutRef } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { cx } from "../ui";

export function MarkdownBlock({ markdown }: { markdown: string }) {
  return (
    <div className="text-[0.98rem] leading-8 text-ink-dim">
      <ReactMarkdown components={markdownComponents} remarkPlugins={[remarkGfm]}>
        {markdown}
      </ReactMarkdown>
    </div>
  );
}

const markdownComponents = {
  a(props: ComponentPropsWithoutRef<"a">) {
    return (
      <a
        {...props}
        className="text-[#f4bf63] underline decoration-1 underline-offset-[0.18em]"
      />
    );
  },
  blockquote(props: ComponentPropsWithoutRef<"blockquote">) {
    return (
      <blockquote
        {...props}
        className="mb-4 border-l-2 border-accent pl-3.5 text-ink last:mb-0"
      />
    );
  },
  code({
    className,
    inline,
    ...props
  }: ComponentPropsWithoutRef<"code"> & { inline?: boolean }) {
    if (inline) {
      return (
        <code
          {...props}
          className="border border-border bg-panel px-1.5 py-0.5 font-mono text-[0.9em] text-[#f4bf63]"
        />
      );
    }

    return <code {...props} className={cx("font-mono text-[0.9em]", className)} />;
  },
  h1(props: ComponentPropsWithoutRef<"h1">) {
    return <h1 {...props} className="mb-3.5 text-2xl font-semibold leading-tight text-ink" />;
  },
  h2(props: ComponentPropsWithoutRef<"h2">) {
    return <h2 {...props} className="mb-3.5 text-xl font-semibold leading-tight text-ink" />;
  },
  h3(props: ComponentPropsWithoutRef<"h3">) {
    return <h3 {...props} className="mb-3.5 text-lg font-semibold leading-tight text-ink" />;
  },
  h4(props: ComponentPropsWithoutRef<"h4">) {
    return <h4 {...props} className="mb-3.5 text-lg font-semibold leading-tight text-ink" />;
  },
  hr(props: ComponentPropsWithoutRef<"hr">) {
    return <hr {...props} className="mb-4 border-border last:mb-0" />;
  },
  li(props: ComponentPropsWithoutRef<"li">) {
    return <li {...props} className="mt-1 first:mt-0" />;
  },
  ol(props: ComponentPropsWithoutRef<"ol">) {
    return <ol {...props} className="mb-4 list-decimal pl-[1.35rem] text-ink last:mb-0" />;
  },
  p(props: ComponentPropsWithoutRef<"p">) {
    return <p {...props} className="mb-4 last:mb-0" />;
  },
  pre(props: ComponentPropsWithoutRef<"pre">) {
    return (
      <pre
        {...props}
        className="mb-4 overflow-x-auto border border-border bg-panel px-4 py-3 text-[0.95rem] text-[#dbe7ff] last:mb-0"
      />
    );
  },
  strong(props: ComponentPropsWithoutRef<"strong">) {
    return <strong {...props} className="font-semibold text-ink" />;
  },
  table(props: ComponentPropsWithoutRef<"table">) {
    return (
      <div className="mb-4 overflow-x-auto last:mb-0">
        <table
          {...props}
          className="w-full border-collapse text-left text-sm text-ink-dim"
        />
      </div>
    );
  },
  td(props: ComponentPropsWithoutRef<"td">) {
    return <td {...props} className="border border-border px-3 py-2.5 align-top" />;
  },
  th(props: ComponentPropsWithoutRef<"th">) {
    return (
      <th
        {...props}
        className="border border-border bg-white/4 px-3 py-2.5 font-semibold text-ink"
      />
    );
  },
  ul(props: ComponentPropsWithoutRef<"ul">) {
    return <ul {...props} className="mb-4 list-disc pl-[1.35rem] text-ink last:mb-0" />;
  },
};
