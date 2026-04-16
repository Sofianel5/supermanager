import { copyLabelClass, copySheetClass } from "../ui";

export function CopyPanel({
  copiedValue,
  label,
  onCopy,
  value,
}: {
  copiedValue: string | null;
  label: string;
  onCopy: (label: string, value: string) => Promise<void>;
  value: string;
}) {
  return (
    <button className={copySheetClass} type="button" onClick={() => onCopy(label, value)}>
      <span className={copyLabelClass}>
        {label} {copiedValue === label ? "copied" : "click to copy"}
      </span>
      <code className="mt-2.5 block break-words font-mono text-[13px] leading-7 text-[#f4bf63]">
        {value}
      </code>
    </button>
  );
}
