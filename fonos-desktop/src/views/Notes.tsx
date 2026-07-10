// Notes view — two-level: notebook list → notebook detail.
// Level 1: Quick Notes section + notebook grid + new notebook creation.
// Level 2: Chronological entries with edit, delete, and export.

import { useState, useEffect, useCallback, useRef } from "react";
import { PinIcon, NotebookIcon } from "../components/Icons";
import {
  listContainers,
  getContainerEntries,
  updateEntry,
  deleteEntry,
  exportNotebookMd,
  exportNotebookJson,
} from "../lib/storage-api";
import type { Container, Entry } from "../lib/storage-api";
import { playAudioFile } from "../lib/api";
import { t, useT } from "../lib/i18n";

// ─── Helpers ──────────────────────────────────────────────────────────────────

function relativeTime(isoDate: string): string {
  const d = new Date(isoDate);
  const now = new Date();
  const today = now.toDateString();
  const yesterday = new Date(now.getTime() - 86400000).toDateString();
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (d.toDateString() === today) return `${t("notes.today")}, ${time}`;
  if (d.toDateString() === yesterday) return `${t("notes.yesterday")}, ${time}`;
  return `${d.toLocaleDateString([], {
    month: "short",
    day: "numeric",
  })}, ${time}`;
}

// Used by relativeTime — keep for future notebook card view
// @ts-ignore unused temporarily
function shortRelative(isoDate: string): string {
  const d = new Date(isoDate);
  const now = new Date();
  const diffMs = now.getTime() - d.getTime();
  const diffDays = Math.floor(diffMs / 86400000);
  if (diffDays === 0) return t("notes.today");
  if (diffDays === 1) return t("notes.yesterday");
  if (diffDays < 7) return `${diffDays}${t("notes.d-ago")}`;
  if (diffDays < 30) return `${Math.floor(diffDays / 7)}${t("notes.w-ago")}`;
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}



// ─── Icons ────────────────────────────────────────────────────────────────────

const BACK_ICON = (
  <svg
    width={16}
    height={16}
    viewBox="0 0 24 24"
    fill="none"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
    stroke="currentColor"
  >
    <polyline points="15 18 9 12 15 6" />
  </svg>
);



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
      className="rounded-lg bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)] px-4 py-3 flex flex-col gap-2"
    >
      {/* Top row: timestamp + actions */}
      <div className="flex items-center gap-2">
        <time
          data-testid="entry-time"
          dateTime={entry.created_at}
          className="text-[11px] text-[rgba(255,255,255,0.35)]"
        >
          {relativeTime(entry.created_at)}
        </time>
        <span className="flex-1" />

        {/* Audio play button (only if audio_ref present AND onPlay provided) */}
        {onPlay && entry.audio_ref && (
          <button
            data-testid="audio-play-btn"
            onClick={() => onPlay(entry.audio_ref!)}
            title={t("notes.play-audio")}
            className="w-[22px] h-[22px] rounded-md flex items-center justify-center text-[rgba(255,255,255,0.3)] hover:text-[var(--accent)] hover:bg-[rgba(242,184,75,0.08)] transition-colors"
          >
            {PLAY_ICON}
          </button>
        )}

        {/* Edit button */}
        <button
          data-testid="edit-entry-btn"
          onClick={handleEditClick}
          title={t("notes.edit")}
          className="w-[22px] h-[22px] rounded-md flex items-center justify-center text-[rgba(255,255,255,0.25)] hover:text-[rgba(255,255,255,0.6)] hover:bg-[rgba(255,255,255,0.05)] transition-colors"
        >
          {PENCIL_ICON}
        </button>

        {/* Delete button */}
        <button
          data-testid="delete-entry-btn"
          onClick={handleDeleteClick}
          title={confirmDelete ? t("notes.confirm-delete") : t("notes.delete")}
          className={[
            "w-[22px] h-[22px] rounded-md flex items-center justify-center transition-colors",
            confirmDelete
              ? "text-[#ef4444] bg-[rgba(239,68,68,0.1)]"
              : "text-[rgba(255,255,255,0.25)] hover:text-[#ef4444] hover:bg-[rgba(239,68,68,0.08)]",
          ].join(" ")}
        >
          {TRASH_ICON}
        </button>
      </div>

      {/* Text content or edit mode */}
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
          className="text-[13px] text-[rgba(255,255,255,0.7)] leading-relaxed whitespace-pre-wrap"
        >
          {displayText || (
            <span className="text-[rgba(255,255,255,0.25)] italic">{t("notes.no-text")}</span>
          )}
        </p>
      )}
    </div>
  );
}

// ─── Notebook Detail view ─────────────────────────────────────────────────────

interface NotebookDetailProps {
  notebook: Container;
  onBack: () => void;
}

function NotebookDetail({ notebook, onBack }: NotebookDetailProps) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [loading, setLoading] = useState(true);
  const [showExportMenu, setShowExportMenu] = useState(false);
  const exportRef = useRef<HTMLDivElement>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const results = await getContainerEntries(notebook.id);
      // Sort chronologically (oldest first)
      const sorted = [...results].sort(
        (a, b) =>
          new Date(a.created_at).getTime() - new Date(b.created_at).getTime()
      );
      setEntries(sorted);
    } catch (e) {
      console.error("getContainerEntries:", e);
      setEntries([]);
    } finally {
      setLoading(false);
    }
  }, [notebook.id]);

  useEffect(() => {
    load();
  }, [load]);

  // Close export menu when clicking outside
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (exportRef.current && !exportRef.current.contains(e.target as Node)) {
        setShowExportMenu(false);
      }
    };
    if (showExportMenu) document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [showExportMenu]);

  const handleEdit = useCallback(
    async (id: number, newText: string) => {
      await updateEntry(id, newText);
      setEntries((prev) =>
        prev.map((e) => (e.id === id ? { ...e, processed_text: newText } : e))
      );
    },
    []
  );

  const handleDelete = useCallback(async (id: number) => {
    await deleteEntry(id);
    setEntries((prev) => prev.filter((e) => e.id !== id));
  }, []);

  const handlePlay = useCallback((audioRef: string) => {
    playAudioFile(audioRef).catch((e) => console.error("playAudioFile:", e));
  }, []);

  const handleExportMd = async () => {
    setShowExportMenu(false);
    try {
      await exportNotebookMd(notebook.id, "");
    } catch (e) {
      console.error("exportNotebookMd:", e);
    }
  };

  const handleExportJson = async () => {
    setShowExportMenu(false);
    try {
      await exportNotebookJson(notebook.id, "");
    } catch (e) {
      console.error("exportNotebookJson:", e);
    }
  };

  return (
    <div
      data-testid="notebook-detail"
      className="flex flex-col h-full bg-[var(--bg)]"
    >
      {/* Top bar */}
      <div className="flex items-center gap-2 px-4 py-3 border-b border-[rgba(255,255,255,0.05)] flex-shrink-0">
        <button
          data-testid="back-btn"
          onClick={onBack}
          title={t("notes.back-to-notebooks")}
          className="w-[28px] h-[28px] rounded-lg flex items-center justify-center text-[rgba(255,255,255,0.4)] hover:text-[rgba(255,255,255,0.7)] hover:bg-[rgba(255,255,255,0.06)] transition-colors"
        >
          {BACK_ICON}
        </button>

        <h2 className="text-[15px] font-semibold text-[#fafaf9] flex-1 truncate">
          {notebook.title}
        </h2>

        {/* Export dropdown */}
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
      </div>

      {/* Entry list */}
      <div
        data-testid="entry-list"
        className="flex-1 overflow-auto px-4 py-4 flex flex-col gap-3"
      >
        {loading && (
          <div className="text-center text-[rgba(255,255,255,0.2)] text-[12px] py-8">
            {t("notes.loading-entries")}
          </div>
        )}

        {!loading && entries.length === 0 && (
          <div
            data-testid="notebook-empty"
            className="text-center text-[rgba(255,255,255,0.2)] text-[12px] py-12 flex flex-col items-center gap-2"
          >
            <NotebookIcon size={32} className="opacity-30" />
            <p>{t("notes.empty-notebook")}</p>
            <p className="text-[rgba(255,255,255,0.12)] text-[11px]">
              {t("notes.empty-hint")}
            </p>
          </div>
        )}

        {!loading &&
          entries.map((entry) => (
            <EntryItem
              key={entry.id}
              entry={entry}
              onEdit={handleEdit}
              onDelete={handleDelete}
              onPlay={handlePlay}
            />
          ))}
      </div>
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
          (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
        );
        setSelectedEntries(sorted);
      })
      .catch(() => setSelectedEntries([]))
      .finally(() => setLoadingEntries(false));
    // Also refresh all notebook counts
    loadNotebooks();
  }, [selectedId, loadNotebooks]);

  const selectedNotebook = sortedNotebooks.find((nb) => nb.id === selectedId);

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

      <div className="mx-5 border-t border-[rgba(255,255,255,0.04)]" />

      {/* Selected notebook entries */}
      <div className="flex-1 overflow-y-auto px-5 py-3">
        {loading ? (
          <div className="text-[rgba(255,255,255,0.2)] text-[11px] py-4 text-center">{t("notes.loading")}</div>
        ) : loadingEntries ? (
          <div className="text-[rgba(255,255,255,0.2)] text-[11px] py-4 text-center">{t("notes.loading-entries")}</div>
        ) : selectedEntries.length === 0 ? (
          <div className="text-[rgba(255,255,255,0.2)] text-[11px] py-8 text-center">
            {selectedNotebook ? `${t("notes.no-entries-in")} ${selectedNotebook.title}` : t("notes.select-notebook")}
          </div>
        ) : (
          <div className="flex flex-col gap-3">
            {(() => {
              const refresh = () => {
                if (selectedId !== null) {
                  getContainerEntries(selectedId)
                    .then((e) => setSelectedEntries([...e].sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime())))
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
                  <div className="flex flex-col gap-2">
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
  const [view, setView] = useState<"list" | "detail">("list");
  const [selectedNotebook, setSelectedNotebook] = useState<Container | null>(null);

  void selectedNotebook; // detail view uses this

  const handleBack = () => {
    setView("list");
    setSelectedNotebook(null);
  };

  if (view === "detail" && selectedNotebook) {
    return (
      <NotebookDetail notebook={selectedNotebook} onBack={handleBack} />
    );
  }

  return <NotebookList embedded={embedded} initialNotebookId={initialNotebookId} />;
}
