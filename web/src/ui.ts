export function cx(...values: Array<string | false | null | undefined>) {
  return values.filter(Boolean).join(" ");
}

export const pageShellClass =
  "relative z-10 mx-auto w-full max-w-[1180px] px-5 pb-[72px] pt-9";

export const centeredShellClass = `${pageShellClass} flex min-h-screen items-center justify-center`;

export const surfaceClass =
  "rounded-lg border border-border bg-[linear-gradient(180deg,rgba(17,24,37,0.72),rgba(8,12,19,0.88))] backdrop-blur-xl";

export const strongSurfaceClass =
  "rounded-lg border border-border-strong bg-[linear-gradient(180deg,rgba(17,24,37,0.72),rgba(8,12,19,0.88))] backdrop-blur-xl";

export const elevatedSurfaceClass =
  "rounded-lg border border-border-strong bg-[linear-gradient(180deg,rgba(18,25,39,0.94),rgba(7,11,18,0.98))] shadow-float";

export const subduedSurfaceClass =
  "rounded-lg border border-border bg-[rgba(6,9,15,0.74)]";

export const sectionLabelClass =
  "inline-flex items-center gap-2 font-mono text-[11px] font-semibold uppercase text-accent";

export const messageClass = "m-0 text-[0.95rem] leading-7 text-ink-dim";
export const errorMessageClass = "m-0 text-[0.95rem] leading-7 text-danger";

export const fieldLabelClass = "font-mono text-xs uppercase text-ink-muted";

export const inputClass =
  "w-full rounded-lg border border-border bg-panel px-4 py-3 text-base text-ink outline-none transition focus:border-accent";

export const jumboInputClass =
  "w-full rounded-lg border border-border bg-panel px-6 py-5 text-2xl text-ink outline-none transition focus:border-accent sm:text-[1.75rem]";

const actionBaseClass =
  "inline-flex min-h-[46px] items-center justify-center gap-2 rounded-lg border px-[18px] text-center transition duration-150 hover:-translate-y-px disabled:cursor-wait disabled:opacity-70";

export const primaryButtonClass =
  `${actionBaseClass} border-transparent bg-accent font-mono text-[0.82rem] font-bold uppercase text-[#0a0e15] no-underline`;

export const secondaryButtonClass =
  `${actionBaseClass} border-border bg-panel-soft text-ink no-underline`;

export const copySheetClass =
  "w-full rounded-lg border border-border bg-panel p-4 text-left transition duration-150 hover:-translate-y-px";

export const copyLabelClass =
  "flex items-center justify-between gap-3 font-mono text-[11px] uppercase text-ink-muted";

export const pillBaseClass =
  "inline-flex min-h-[30px] items-center rounded-full border px-3 font-mono text-[11px] uppercase";

export const roomMetaClass =
  "mt-3.5 flex flex-wrap items-center gap-3 font-mono text-[0.8rem] text-ink-dim";

export const statusBlockClass = `${surfaceClass} w-full max-w-[420px] p-[22px]`;
export const dialogCardClass = `${elevatedSurfaceClass} grid gap-4 p-[22px]`;
