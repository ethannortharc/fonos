// Name-is-identity picker for the notebook widget (spec §1): one text input
// that both binds existing notebooks and names new ones. Selection semantics
// live in lib/notebookSelection.ts; this component only owns the popover.
// It intentionally has no "rename" semantics — renaming a notebook is a
// management action on the notebook itself, not on the widget binding.
// The input text is seeded from props once per mount — callers that reuse one
// instance across different widgets must remount it (key={form.id}), matching
// WidgetForm's own value.id resync contract.

import { useEffect, useRef, useState } from "react";
import { t, useT } from "../../lib/i18n";
import type { Container } from "../../lib/storage-api";
import {
  resolveNotebookInput,
  type NotebookSelection,
} from "../../lib/notebookSelection";
import { inputClass } from "./constants";

export default function NotebookCombobox({
  containerId,
  pendingTitle,
  notebooks,
  onChange,
}: {
  /** Currently bound container id (0 = Quick Note sentinel). */
  containerId: number;
  /** Pending create title (props.container_title), if the user already typed one. */
  pendingTitle: string;
  notebooks: Container[];
  onChange: (sel: NotebookSelection) => void;
}) {
  useT();
  const bound = notebooks.find((c) => c.id === containerId);
  const [text, setText] = useState(
    pendingTitle || (containerId === 0 ? "" : bound?.title ?? "")
  );
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  // Close the popover on outside click (same pattern as Notes' export menu).
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    if (open) document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const sel = resolveNotebookInput(text, notebooks);
  const pick = (c: Container | null) => {
    // null = the Quick Note default row.
    setText(c ? c.title : "");
    setOpen(false);
    onChange(
      c ? { kind: "existing", container_id: c.id } : { kind: "existing", container_id: 0 }
    );
  };

  return (
    <div ref={rootRef} className="relative flex flex-col gap-1">
      <input
        type="text"
        data-testid="notebook-combobox-input"
        value={text}
        placeholder={t("widgets.notebook.ph")}
        onFocus={() => setOpen(true)}
        onChange={(e) => {
          setText(e.target.value);
          setOpen(true);
          onChange(resolveNotebookInput(e.target.value, notebooks));
        }}
        className={inputClass}
      />
      {open && (
        <div className="absolute left-0 right-0 top-full mt-1 max-h-[180px] overflow-y-auto bg-[#242220] border border-[rgba(255,255,255,0.1)] rounded-lg shadow-xl z-50">
          <button
            data-testid="notebook-option-0"
            onMouseDown={(e) => { e.preventDefault(); pick(null); }}
            className="w-full px-3 py-2 text-left text-[12px] text-[rgba(255,255,255,0.7)] hover:bg-[rgba(255,255,255,0.06)] transition-colors"
          >
            {t("widgets.notebook.quick")}
          </button>
          {notebooks.map((c) => (
            <button
              key={c.id}
              data-testid={`notebook-option-${c.id}`}
              onMouseDown={(e) => { e.preventDefault(); pick(c); }}
              className="w-full px-3 py-2 text-left text-[12px] text-[rgba(255,255,255,0.7)] hover:bg-[rgba(255,255,255,0.06)] transition-colors"
            >
              {c.title}
            </button>
          ))}
        </div>
      )}
      {sel.kind === "create" && (
        <div
          data-testid="notebook-combobox-hint"
          className="text-[10px] text-[rgba(242,184,75,0.7)]"
        >
          {t("widgets.notebook.will-create").replace("{name}", sel.title)}
        </div>
      )}
    </div>
  );
}
