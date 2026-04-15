import { useEffect, useRef } from "react";

interface CreateRoomDialogProps {
  error?: string | null;
  isCreating?: boolean;
  name: string;
  onClose(): void;
  onCreate(): void;
  onNameChange(name: string): void;
}

export function CreateRoomDialog({
  error,
  isCreating = false,
  name,
  onClose,
  onCreate,
  onNameChange,
}: CreateRoomDialogProps) {
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  return (
    <div className="dialog-backdrop">
      <div
        className="dialog-card create-room-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="create-room-dialog-title"
      >
        <div>
          <div className="section-label">Create room</div>
          <h2 id="create-room-dialog-title">New room</h2>
          <p className="message create-room-dialog__copy">
            Give the room a name. You can add the rest after it exists.
          </p>
        </div>

        <form
          className="create-room-dialog__form"
          onSubmit={(event) => {
            event.preventDefault();
            onCreate();
          }}
        >
          <label className="create-room-dialog__label" htmlFor="create-room-name">
            Room name
          </label>
          <input
            ref={inputRef}
            id="create-room-name"
            name="room-name"
            type="text"
            autoComplete="off"
            spellCheck={false}
            value={name}
            onChange={(event) => onNameChange(event.target.value)}
          />

          {error && <p className="message message--error">{error}</p>}

          <div className="dialog-actions">
            <button className="secondary-button" type="button" onClick={onClose}>
              Cancel
            </button>
            <button className="primary-button" type="submit" disabled={isCreating}>
              {isCreating ? "Creating..." : "Create room"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
