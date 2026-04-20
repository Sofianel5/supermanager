import { useEffect, useRef } from "react";
import {
  dialogCardClass,
  errorMessageClass,
  fieldLabelClass,
  inputClass,
  messageClass,
  primaryButtonClass,
  secondaryButtonClass,
  sectionLabelClass,
} from "../../ui";

interface CreateProjectDialogProps {
  error?: string | null;
  isCreating?: boolean;
  name: string;
  onClose(): void;
  onCreate(): void;
  onNameChange(name: string): void;
}

export function CreateProjectDialog({
  error,
  isCreating = false,
  name,
  onClose,
  onCreate,
  onNameChange,
}: CreateProjectDialogProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/55 p-5 backdrop-blur-md">
      <div
        className={`${dialogCardClass} w-full max-w-[460px]`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="create-project-dialog-title"
      >
        <div>
          <div className={sectionLabelClass}>Create project</div>
          <h2
            className="mt-4 text-4xl font-semibold leading-none text-ink sm:text-[2.8rem]"
            id="create-project-dialog-title"
          >
            New project
          </h2>
          <p className={`${messageClass} mt-3`}>
            Give the project a name. You can add the rest after it exists.
          </p>
        </div>

        <form
          className="grid gap-3.5"
          onSubmit={(event) => {
            event.preventDefault();
            onCreate();
          }}
        >
          <label className={fieldLabelClass} htmlFor="create-project-name">
            Project name
          </label>
          <input
            className={inputClass}
            ref={inputRef}
            id="create-project-name"
            name="project-name"
            type="text"
            autoComplete="off"
            spellCheck={false}
            value={name}
            onChange={(event) => onNameChange(event.target.value)}
          />

          {error && <p className={errorMessageClass}>{error}</p>}

          <div className="grid gap-3 sm:grid-cols-2">
            <button className={secondaryButtonClass} type="button" onClick={onClose}>
              Cancel
            </button>
            <button className={primaryButtonClass} type="submit" disabled={isCreating}>
              {isCreating ? "Creating..." : "Create project"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
