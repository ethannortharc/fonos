// Notes view — a document-flow reading experience: notebooks as flowing,
// borderless journal pages (oldest-first, day-grouped, hover-revealed actions).

import { useState, useEffect, useCallback, useRef } from "react";
import { PinIcon, NotebookIcon } from "../components/Icons";
import {
  listContainers,
  getContainerEntries,
  updateEntry,
  deleteEntry,
  deleteContainer,
  exportNotebookMd,
  exportNotebookJson,
} from "../lib/storage-api";
import type { Container, Entry } from "../lib/storage-api";
import { playAudioFile } from "../lib/api";
import { t, useT } from "../lib/i18n";

// ─── Helpers ──────────────────────────────────────────────────────────────────

/** Paragraph-tail timestamp: just the clock time — the day is already given
 *  by the group header above. */
function timeOnly(isoDate: string): string {
  return new Date(isoDate).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}


// ─── Icons ────────────────────────────────────────────────────────────────────

const PENCIL_ICON = (
  <svg
    width={13}
    height={13}
    viewBox="0 0 24 24"
    fill="none"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
    stroke="currentColor"
  >
    <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
    <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
  </svg>
);

const TRASH_ICON = (
  <svg
    width={13}
    height={13}
    viewBox="0 0 24 24"
    fill="none"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
    stroke="currentColor"
  >
    <polyline points="3 6 5 6 21 6" />
    <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
    <path d="M10 11v6" />
    <path d="M14 11v6" />
    <path d="M9 6V4h6v2" />
  </svg>
);

const PLAY_ICON = (
  <svg
    width={13}
    height={13}
    viewBox="0 0 24 24"
    fill="none"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
    stroke="currentColor"
  >
    <polygon points="5 3 19 12 5 21 5 3" />
  </svg>
);

const EXPORT_ICON = (
  <svg
    width={14}
    height={14}
    viewBox="0 0 24 24"
    fill="none"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
    stroke="currentColor"
  >
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <polyline points="7 10 12 15 17 10" />
    <line x1="12" y1="15" x2="12" y2="3" />
  </svg>
);

const CHEVRON_ICON = (
  <svg
    width={12}
    height={12}
    viewBox="0 0 24 24"
    fill="none"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
    stroke="currentColor"
  >
    <polyline points="6 9 12 15 18 9" />
  </svg>
);

// ─── Entry item (used in notebook detail) ─────────────────────────────────────

interface EntryItemProps {
  entry: Entry;
  onEdit: (id: number, newText: string) => Promise<void>;
  onDelete: (id: number) => Promise<void>;
  onPlay?: (audioRef: string) => void;
}

/** System notebooks are recreated lazily by the app — deleting them would
 *  just resurrect an empty copy, so the UI doesn't offer it (spec §3). */
const SYSTEM_NOTEBOOKS = ["Quick Note", "Text Actions"];

/** Day bucket label for journal-style grouping: Today / Yesterday / Mar 5. */
function dayLabel(isoDate: string): string {
  try {
    const d = new Date(isoDate);
    const now = new Date();
    if (d.toDateString() === now.toDateString()) return t("notes.today");
    const y = new Date(now.getTime() - 86400000);
    if (d.toDateString() === y.toDateString()) return t("notes.yesterday");
    const opts: Intl.DateTimeFormatOptions =
      d.getFullYear() === now.getFullYear()
        ? { month: "short", day: "numeric" }
        : { month: "short", day: "numeric", year: "numeric" };
    return d.toLocaleDateString([], opts);
  } catch {
    return isoDate;
  }
}

function EntryItem({ entry, onEdit, onDelete, onPlay }: EntryItemProps) {
  const [editMode, setEditMode] = useState(false);
  const [editText, setEditText] = useState(entry.processed_text || entry.raw_text);
  const [saving, setSaving] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const displayText = entry.processed_text || entry.raw_text;

  const handleEditClick = () => {
    setEditText(entry.processed_text || entry.raw_text);
    setEditMode(true);
    setTimeout(() => {
      textareaRef.current?.focus();
      textareaRef.current?.select();
    }, 50);
  };

  const handleSave = async () => {
    if (saving) return;
    setSaving(true);
    try {
      await onEdit(entry.id, editText);
      setEditMode(false);
    } catch (e) {
      console.error("updateEntry:", e);
    } finally {
      setSaving(false);
    }
  };

  const handleCancel = () => {
    setEditMode(false);
    setEditText(entry.processed_text || entry.raw_text);
  };

  const handleDeleteClick = () => {
    if (!confirmDelete) {
      setConfirmDelete(true);
      setTimeout(() => setConfirmDelete(false), 3000);
      return;
    }
    onDelete(entry.id).catch((e) => console.error("deleteEntry:", e));
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Escape") {
      handleCancel();
    } else if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      handleSave();
    }
  };

  return (
    <div
      data-testid="entry-card"
      className="group relative rounded-md px-2 py-1.5 -mx-2 hover:bg-[rgba(255,255,255,0.025)] transition-colors"
    >
      {editMode ? (
        <div className="flex flex-col gap-2">
          <textarea
            ref={textareaRef}
            data-testid="entry-editor"
            value={editText}
            onChange={(e) => setEditText(e.target.value)}
            onKeyDown={handleKeyDown}
            rows={4}
            className="w-full bg-[rgba(255,255,255,0.04)] border border-[rgba(255,255,255,0.1)] rounded-lg px-3 py-2 text-[13px] text-[rgba(255,255,255,0.8)] resize-none outline-none focus:border-[rgba(242,184,75,0.4)] transition-colors"
          />
          <div className="flex gap-2 justify-end">
            <button
              onClick={handleCancel}
              className="px-3 py-1 text-[11px] text-[rgba(255,255,255,0.4)] hover:text-[rgba(255,255,255,0.6)] transition-colors"
            >
              {t("notes.cancel")}
            </button>
            <button
              data-testid="save-entry-btn"
              onClick={handleSave}
              disabled={saving}
              className="px-3 py-1 text-[11px] bg-[rgba(242,184,75,0.12)] text-[var(--accent)] hover:bg-[rgba(242,184,75,0.2)] rounded-lg transition-colors disabled:opacity-50"
            >
              {saving ? t("notes.saving") : t("notes.save")}
            </button>
          </div>
        </div>
      ) : (
        <p
          data-testid="entry-text"
          className="text-[13px] text-[rgba(255,255,255,0.72)] leading-relaxed whitespace-pre-wrap"
        >
          {displayText || (
            <span className="text-[rgba(255,255,255,0.25)] italic">{t("notes.no-text")}</span>
          )}
          <time
            data-testid="entry-time"
            dateTime={entry.created_at}
            className="ml-2 text-[10px] text-[rgba(255,255,255,0.22)] whitespace-nowrap select-none"
          >
            · {timeOnly(entry.created_at)}
          </time>
        </p>
      )}

      {/* Hover actions — CSS-only reveal so paragraphs stay clean. */}
      {!editMode && (
        <div className="absolute right-0 -top-2 hidden group-hover:flex items-center gap-0.5 rounded-md bg-[#242220] border border-[rgba(255,255,255,0.08)] px-1 py-0.5 shadow-lg">
          {onPlay && entry.audio_ref && (
            <button
              data-testid="audio-play-btn"
              onClick={() => onPlay(entry.audio_ref!)}
              title={t("notes.play-audio")}
              className="w-[22px] h-[22px] rounded flex items-center justify-center text-[rgba(255,255,255,0.3)] hover:text-[var(--accent)] transition-colors"
            >
              {PLAY_ICON}
            </button>
          )}
          <button
            data-testid="edit-entry-btn"
            onClick={handleEditClick}
            title={t("notes.edit")}
            className="w-[22px] h-[22px] rounded flex items-center justify-center text-[rgba(255,255,255,0.35)] hover:text-[rgba(255,255,255,0.7)] transition-colors"
          >
            {PENCIL_ICON}
          </button>
          <button
            data-testid="delete-entry-btn"
            onClick={handleDeleteClick}
            title={confirmDelete ? t("notes.confirm-delete") : t("notes.delete")}
            className={[
              "w-[22px] h-[22px] rounded flex items-center justify-center transition-colors",
              confirmDelete
                ? "text-[#ef4444] bg-[rgba(239,68,68,0.1)]"
                : "text-[rgba(255,255,255,0.35)] hover:text-[#ef4444]",
            ].join(" ")}
          >
            {TRASH_ICON}
          </button>
        </div>
      )}
    </div>
  );
}

// ─── Export menu ──────────────────────────────────────────────────────────────

/** Export dropdown for the selected notebook — lifted verbatim out of the
 *  removed NotebookDetail so the list view's header can host it. */
function ExportMenu({ notebookId }: { notebookId: number }) {
  const [showExportMenu, setShowExportMenu] = useState(false);
  const exportRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (exportRef.current && !exportRef.current.contains(e.target as Node)) {
        setShowExportMenu(false);
      }
    };
    if (showExportMenu) document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [showExportMenu]);

  const handleExportMd = async () => {
    setShowExportMenu(false);
    try {
      await exportNotebookMd(notebookId, "");
    } catch (e) {
      console.error("exportNotebookMd:", e);
    }
  };

  const handleExportJson = async () => {
    setShowExportMenu(false);
    try {
      await exportNotebookJson(notebookId, "");
    } catch (e) {
      console.error("exportNotebookJson:", e);
    }
  };

  return (
    <div className="relative" ref={exportRef}>
      <button
        data-testid="export-notebook-btn"
        onClick={() => setShowExportMenu((v) => !v)}
        title={t("notes.export-notebook")}
        className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-[11px] text-[rgba(255,255,255,0.4)] hover:text-[rgba(255,255,255,0.7)] hover:bg-[rgba(255,255,255,0.06)] transition-colors"
      >
        {EXPORT_ICON}
        <span>{t("notes.export")}</span>
        {CHEVRON_ICON}
      </button>

      {showExportMenu && (
        <div className="absolute right-0 top-full mt-1 w-[160px] bg-[#242220] border border-[rgba(255,255,255,0.1)] rounded-lg shadow-xl z-50 overflow-hidden">
          <button
            data-testid="export-md"
            onClick={handleExportMd}
            className="w-full px-3 py-2.5 text-left text-[12px] text-[rgba(255,255,255,0.7)] hover:bg-[rgba(255,255,255,0.06)] transition-colors"
          >
            {t("notes.export-md")}
          </button>
          <button
            data-testid="export-json"
            onClick={handleExportJson}
            className="w-full px-3 py-2.5 text-left text-[12px] text-[rgba(255,255,255,0.7)] hover:bg-[rgba(255,255,255,0.06)] transition-colors"
          >
            {t("notes.export-json")}
          </button>
        </div>
      )}
    </div>
  );
}

// ─── Notebook card ────────────────────────────────────────────────────────────




// ─── New Notebook form ────────────────────────────────────────────────────────

// ─── Notebook list (Level 1) ──────────────────────────────────────────────────

function NotebookList({ embedded, initialNotebookId }: { embedded?: boolean; initialNotebookId?: number }) {
  const [notebooks, setNotebooks] = useState<Container[]>([]);
  const [entryCounts, setEntryCounts] = useState<Record<number, number>>({});
  const [loading, setLoading] = useState(true);

  const loadNotebooks = useCallback(async () => {
    setLoading(true);
    try {
      const containers = await listContainers();
      // Filter to notebooks only
      const nbs = containers.filter((c) => c.container_type === "notebook");
      setNotebooks(nbs);

      // Load entry counts and previews for each notebook
      const counts: Record<number, number> = {};
      await Promise.all(
        nbs.map(async (nb) => {
          try {
            const nbEntries = await getContainerEntries(nb.id);
            counts[nb.id] = nbEntries.length;
          } catch {
            counts[nb.id] = 0;
          }
        })
      );
      setEntryCounts(counts);
    } catch (e) {
      console.error("listContainers:", e);
      setNotebooks([]);
    } finally {
      setLoading(false);
    }
  }, []);

  // Sort: Quick Note first, then alphabetical
  const sortedNotebooks = [...notebooks].sort((a, b) => {
    if (a.title === "Quick Note") return -1;
    if (b.title === "Quick Note") return 1;
    return a.title.localeCompare(b.title);
  });

  // Selected notebook — default to Quick Note
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [selectedEntries, setSelectedEntries] = useState<Entry[]>([]);
  const [loadingEntries, setLoadingEntries] = useState(false);

  // Auto-select Quick Note when notebooks load
  useEffect(() => {
    loadNotebooks();
  }, [loadNotebooks]);

  useEffect(() => {
    if (selectedId === null && sortedNotebooks.length > 0) {
      const preferred =
        initialNotebookId != null && sortedNotebooks.some((nb) => nb.id === initialNotebookId)
          ? initialNotebookId
          : sortedNotebooks[0].id;
      setSelectedId(preferred);
    }
  }, [sortedNotebooks, selectedId, initialNotebookId]);

  // Load entries when selected notebook changes — also refresh counts for all notebooks
  useEffect(() => {
    if (selectedId === null) return;
    setLoadingEntries(true);
    getContainerEntries(selectedId)
      .then((entries) => {
        const sorted = [...entries].sort(
          (a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
        );
        setSelectedEntries(sorted);
      })
      .catch(() => setSelectedEntries([]))
      .finally(() => setLoadingEntries(false));
    // Also refresh all notebook counts
    loadNotebooks();
  }, [selectedId, loadNotebooks]);

  const selectedNotebook = sortedNotebooks.find((nb) => nb.id === selectedId);

  const [confirmNbDelete, setConfirmNbDelete] = useState(false);
  useEffect(() => setConfirmNbDelete(false), [selectedId]);

  const handleDeleteNotebook = async () => {
    if (!selectedNotebook) return;
    if (!confirmNbDelete) {
      setConfirmNbDelete(true);
      setTimeout(() => setConfirmNbDelete(false), 3000);
      return;
    }
    try {
      await deleteContainer(selectedNotebook.id);
      setConfirmNbDelete(false);
      setSelectedId(null); // auto-select effect re-picks the first notebook
      await loadNotebooks();
    } catch (e) {
      console.error("deleteContainer:", e);
    }
  };

  const scrollRef = useRef<HTMLDivElement>(null);
  // A real notebook opens at its latest page: pin to bottom whenever the
  // selected notebook's entries finish loading.
  useEffect(() => {
    if (loadingEntries) return;
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [selectedId, loadingEntries]);

  return (
    <div
      data-testid="notes-view"
      className="flex flex-col h-full bg-[var(--bg)]"
    >
      {/* Header (hidden when embedded in the History view) */}
      {!embedded && (
        <div className="flex items-center justify-between px-5 py-3 flex-shrink-0">
          <h2 className="text-[13px] font-semibold text-[#fafaf9]">{t("notes.title")}</h2>
        </div>
      )}

      {/* Notebook tabs — horizontal scroll */}
      <div className="flex-shrink-0 px-5 pb-2">
        <div className="flex items-center gap-1.5 overflow-x-auto scrollbar-none">
          {sortedNotebooks.map((nb) => (
            <button
              key={nb.id}
              onClick={() => setSelectedId(nb.id)}
              className={[
                "flex-shrink-0 px-3 py-1.5 rounded-full text-[10px] font-medium transition-all border",
                nb.id === selectedId
                  ? "bg-[rgba(242,184,75,0.12)] border-[rgba(242,184,75,0.25)] text-[var(--accent)]"
                  : "bg-[rgba(255,255,255,0.03)] border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.35)] hover:border-[rgba(255,255,255,0.12)]",
              ].join(" ")}
            >
              <span className="inline-flex items-center gap-1">{nb.title === "Quick Note" ? <PinIcon size={10} /> : <NotebookIcon size={10} />}{nb.title}</span>
              <span className="ml-1 opacity-50">
                {nb.id === selectedId ? selectedEntries.length : (entryCounts[nb.id] ?? 0)}
              </span>
            </button>
          ))}
        </div>
      </div>

      {selectedNotebook && (
        <div className="flex items-center justify-end gap-1 px-5 pb-1.5">
          <ExportMenu notebookId={selectedNotebook.id} />
          {!SYSTEM_NOTEBOOKS.includes(selectedNotebook.title) && (
            <button
              data-testid="delete-notebook-btn"
              onClick={handleDeleteNotebook}
              title={t("notes.delete-notebook")}
              className={[
                "flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-[11px] transition-colors",
                confirmNbDelete
                  ? "text-[#ef4444] bg-[rgba(239,68,68,0.1)]"
                  : "text-[rgba(255,255,255,0.4)] hover:text-[#ef4444] hover:bg-[rgba(239,68,68,0.08)]",
              ].join(" ")}
            >
              {TRASH_ICON}
              <span>{confirmNbDelete ? t("notes.delete-notebook-confirm") : t("notes.delete-notebook")}</span>
            </button>
          )}
        </div>
      )}

      <div className="mx-5 border-t border-[rgba(255,255,255,0.04)]" />

      {/* Selected notebook entries */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-5 py-3">
        {loading ? (
          <div className="text-[rgba(255,255,255,0.2)] text-[11px] py-4 text-center">{t("notes.loading")}</div>
        ) : loadingEntries ? (
          <div className="text-[rgba(255,255,255,0.2)] text-[11px] py-4 text-center">{t("notes.loading-entries")}</div>
        ) : selectedEntries.length === 0 ? (
          <div className="text-[rgba(255,255,255,0.2)] text-[11px] py-8 text-center">
            {selectedNotebook ? `${t("notes.no-entries-in")} ${selectedNotebook.title}` : t("notes.select-notebook")}
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            {(() => {
              const refresh = () => {
                if (selectedId !== null) {
                  getContainerEntries(selectedId)
                    .then((e) => setSelectedEntries([...e].sort((a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime())))
                    .catch(() => {});
                  loadNotebooks(); // refresh counts
                }
              };
              // Journal rhythm: bucket entries under day headers.
              const groups: { label: string; items: typeof selectedEntries }[] = [];
              for (const entry of selectedEntries) {
                const label = dayLabel(entry.created_at);
                const last = groups[groups.length - 1];
                if (last && last.label === label) last.items.push(entry);
                else groups.push({ label, items: [entry] });
              }
              return groups.map((g) => (
                <div key={g.label}>
                  <div className="flex items-center gap-2 mb-1.5">
                    <span className="text-[9px] font-semibold uppercase tracking-wider text-[rgba(255,255,255,0.28)]">
                      {g.label}
                    </span>
                    <div className="flex-1 border-t border-[rgba(255,255,255,0.04)]" />
                    <span className="text-[9px] text-[rgba(255,255,255,0.15)]">{g.items.length}</span>
                  </div>
                  <div className="flex flex-col gap-0.5">
                    {g.items.map((entry) => (
                      <EntryItem
                        key={entry.id}
                        entry={entry}
                        onEdit={async (id, text) => { await updateEntry(id, text); refresh(); }}
                        onDelete={async (id) => { await deleteEntry(id); refresh(); }}
                      />
                    ))}
                  </div>
                </div>
              ));
            })()}
          </div>
        )}
      </div>
    </div>
  );
}

// ─── Notes root (manages list/detail state) ───────────────────────────────────

export default function Notes({
  embedded,
  initialNotebookId,
}: {
  embedded?: boolean;
  initialNotebookId?: number;
} = {}) {
  useT();
  return <NotebookList embedded={embedded} initialNotebookId={initialNotebookId} />;
}
