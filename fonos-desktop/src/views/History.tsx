// History — unified browsing over everything captured (issue: menu consolidation).
// One entry point replaces Recent / Search / Notes / Meetings tabs:
//   search box (FTS) + type filter chips + type-appropriate content:
//     all|dictation|agent → unified timeline (meetings collapsed to one card)
//     note               → embedded Notes browser (notebook tabs + entries)
//     meeting            → embedded Meetings list; detail via stack push

import { useState, useEffect, useCallback, useMemo, useRef, type MouseEvent as ReactMouseEvent } from "react";
import { listEntries, listContainers, searchEntries, deleteEntry, updateEntryText } from "../lib/storage-api";
import { playAudioFile, stopPlayback, getConfig, saveConfig } from "../lib/api";
import { t, useT, type TKey } from "../lib/i18n";
import type { Entry, Container, SourceType } from "../lib/storage-api";
import type { AppConfig, VocabBook, VocabRule } from "../types";
import Notes from "./Notes";
import Meetings from "./Meetings";
import { MeetingDetailView } from "./Meetings";

const PAGE_SIZE = 30;
const SEARCH_DEBOUNCE_MS = 250;

export type HistoryFilter = "all" | SourceType;

const FILTERS: { id: HistoryFilter; label: TKey }[] = [
  { id: "all", label: "history.filter.all" },
  { id: "dictation", label: "history.filter.dictation" },
  { id: "note", label: "history.filter.notes" },
  { id: "meeting", label: "history.filter.meetings" },
  { id: "listen", label: "history.filter.listen" },
  { id: "agent", label: "history.filter.agent" },
];

const STRIPE_COLOR: Record<string, string> = {
  dictation: "#a8a29e",
  agent: "#c084fc",
  note: "#4ade80",
  meeting: "#fbbf24",
  listen: "#7dd3fc",
};

function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    const now = new Date();
    const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    if (d.toDateString() === now.toDateString()) return `${t("history.today")} ${time}`;
    const y = new Date(now.getTime() - 86400000);
    if (d.toDateString() === y.toDateString()) return `${t("history.yesterday")} ${time}`;
    return `${d.toLocaleDateString([], { month: "short", day: "numeric" })} ${time}`;
  } catch {
    return iso;
  }
}

function entryText(e: Entry): string {
  return e.processed_text || e.raw_text || "";
}

function preview(text: string, max = 150): string {
  return text.length > max ? text.slice(0, max) + "…" : text;
}

/** Snippet centred on the first match so search results show WHY they matched. */
function makeSnippet(text: string, query: string): { before: string; match: string; after: string } {
  const lower = text.toLowerCase();
  const candidates = [query, ...query.split(/\s+/).filter((t) => t.length >= 3)];
  for (const c of candidates) {
    const idx = lower.indexOf(c.toLowerCase());
    if (idx >= 0) {
      const start = Math.max(0, idx - 60);
      const end = Math.min(text.length, idx + c.length + 90);
      return {
        before: (start > 0 ? "…" : "") + text.slice(start, idx),
        match: text.slice(idx, idx + c.length),
        after: text.slice(idx + c.length, end) + (end < text.length ? "…" : ""),
      };
    }
  }
  return { before: "", match: "", after: preview(text) };
}

// ─── Timeline item model: meetings collapse into one card per session ─────────

type TimelineItem =
  | { kind: "entry"; entry: Entry }
  | { kind: "meeting"; container: Container | null; latest: Entry; segments: number };

function buildTimeline(entries: Entry[], containers: Map<number, Container>): TimelineItem[] {
  const items: TimelineItem[] = [];
  const seenMeetings = new Set<number>();
  for (const e of entries) {
    if (e.source_type === "meeting" && e.container_id != null) {
      if (seenMeetings.has(e.container_id)) continue;
      seenMeetings.add(e.container_id);
      items.push({
        kind: "meeting",
        container: containers.get(e.container_id) ?? null,
        latest: e,
        segments: entries.filter((x) => x.container_id === e.container_id).length,
      });
    } else {
      items.push({ kind: "entry", entry: e });
    }
  }
  return items;
}

// ─── Component ────────────────────────────────────────────────────────────────

export default function History({
  preset,
}: {
  /** External navigation (float pill / tray) can preset the filter. */
  preset?: { filter: HistoryFilter; nonce: number };
}) {
  useT();
  const [filter, setFilter] = useState<HistoryFilter>(preset?.filter ?? "all");
  const [query, setQuery] = useState("");
  const [openMeeting, setOpenMeeting] = useState<Container | null>(null);
  const [notesRefresh, setNotesRefresh] = useState(0);

  // Timeline state
  const [entries, setEntries] = useState<Entry[]>([]);
  const [containers, setContainers] = useState<Map<number, Container>>(new Map());
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [expandedId, setExpandedId] = useState<number | null>(null);

  // Search state
  const [results, setResults] = useState<Entry[]>([]);
  const [searching, setSearching] = useState(false);
  const [searched, setSearched] = useState(false);

  // Correction capture (issue #31): select mis-transcribed text → rule/term.
  const [menu, setMenu] = useState<CorrectionAnchor | null>(null);
  const [popover, setPopover] = useState<CorrectionAnchor | null>(null);

  const openCorrectMenu = useCallback((entry: Entry, selection: string, rect: DOMRect) => {
    setPopover(null);
    setMenu({ entry, selection, rect });
  }, []);

  // Apply a saved correction in place so the fix is visible immediately.
  const applyCorrection = useCallback((entryId: number, newText: string) => {
    setEntries((prev) => prev.map((e) => (e.id === entryId ? { ...e, processed_text: newText } : e)));
    setResults((prev) => prev.map((e) => (e.id === entryId ? { ...e, processed_text: newText } : e)));
  }, []);

  // Escape closes the menu / popover (click-outside is handled by their backdrops).
  useEffect(() => {
    if (!menu && !popover) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") { setMenu(null); setPopover(null); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [menu, popover]);

  // External preset (navigate-tab from pill/tray)
  useEffect(() => {
    if (!preset) return;
    setFilter(preset.filter);
    setQuery("");
    setOpenMeeting(null);
  }, [preset]);

  const loadContainers = useCallback(async () => {
    try {
      const all = await listContainers();
      setContainers(new Map(all.map((c) => [c.id, c])));
    } catch {
      /* non-Tauri env */
    }
  }, []);

  useEffect(() => {
    loadContainers();
  }, [loadContainers]);

  const loadTimeline = useCallback(async (p: number, f: HistoryFilter) => {
    setLoading(true);
    setError("");
    try {
      const sourceFilter = f === "all" ? undefined : f;
      const rows = await listEntries(PAGE_SIZE, p * PAGE_SIZE, sourceFilter);
      setEntries(rows);
    } catch (err) {
      setError(String(err));
      setEntries([]);
    } finally {
      setLoading(false);
    }
  }, []);

  const timelineActive = !query.trim() && (filter === "all" || filter === "dictation" || filter === "agent" || filter === "listen");

  useEffect(() => {
    if (!timelineActive) return;
    setPage(0);
    setExpandedId(null);
    loadTimeline(0, filter);
  }, [filter, timelineActive, loadTimeline]);

  // Debounced search with stale-response guard
  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setResults([]);
      setSearched(false);
      setSearching(false);
      return;
    }
    setSearching(true);
    let cancelled = false;
    const timer = setTimeout(async () => {
      try {
        const found = await searchEntries(q, 50);
        if (cancelled) return;
        setResults(found);
        setError("");
      } catch (err) {
        if (cancelled) return;
        setError(String(err));
        setResults([]);
      } finally {
        if (!cancelled) {
          setSearching(false);
          setSearched(true);
          setExpandedId(null);
        }
      }
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query]);

  const handleDelete = async (id: number) => {
    try {
      // If the deleted item is a listen entry it may be playing right now.
      const victim = entries.find((e) => e.id === id) ?? results.find((e) => e.id === id);
      if (victim?.source_type === "listen") {
        stopPlayback().catch(() => {});
      }
      await deleteEntry(id);
      setEntries((prev) => prev.filter((e) => e.id !== id));
      setResults((prev) => prev.filter((e) => e.id !== id));
    } catch (err) {
      setError(String(err));
    }
  };

  const handleCopy = (text: string) => {
    navigator.clipboard?.writeText(text).catch(() => {});
  };

  const openNotebookOf = (entry: Entry) => {
    // Jump into the Notes browser with the entry's notebook selected.
    setQuery("");
    setFilter("note");
    setNotesRefresh((n) => n + 1);
    setNotesInitialId(entry.container_id);
  };
  const [notesInitialId, setNotesInitialId] = useState<number | null>(null);

  const openMeetingOf = (entry: Entry) => {
    const c = entry.container_id != null ? containers.get(entry.container_id) : null;
    if (c) setOpenMeeting(c);
  };

  const timeline = useMemo(
    () => buildTimeline(entries, containers),
    [entries, containers]
  );

  // ── Stack: meeting detail takes over the whole view ──
  if (openMeeting) {
    return (
      <MeetingDetailView
        meeting={openMeeting}
        onBack={() => setOpenMeeting(null)}
        onDeleted={() => {
          setOpenMeeting(null);
          loadTimeline(page, filter);
        }}
      />
    );
  }

  const q = query.trim();

  return (
    <div className="flex flex-col h-full bg-[#1a1917]">
      {/* Header: title + search */}
      <div className="flex items-center gap-3 px-5 pt-4 pb-2 flex-shrink-0">
        <h2 className="text-[13px] font-semibold text-[#fafaf9] flex-shrink-0">{t("nav.history")}</h2>
        <div className="flex-1 flex items-center gap-2 rounded-lg border border-[rgba(255,255,255,0.08)] bg-[rgba(255,255,255,0.03)] px-2.5 py-1.5">
          <svg width={12} height={12} viewBox="0 0 24 24" fill="none" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" className="stroke-[rgba(255,255,255,0.3)] flex-shrink-0">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            data-testid="history-search"
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Escape") setQuery(""); }}
            placeholder={t("history.search")}
            spellCheck={false}
            className="flex-1 bg-transparent outline-none border-none text-[#fafaf9] text-[11px] placeholder:text-[rgba(255,255,255,0.2)]"
          />
          {query && (
            <button
              onClick={() => setQuery("")}
              className="text-[rgba(255,255,255,0.3)] hover:text-[rgba(255,255,255,0.6)] text-[12px] leading-none"
            >
              ×
            </button>
          )}
        </div>
      </div>

      {/* Filter chips */}
      <div className="flex gap-1.5 px-5 pb-2.5 flex-shrink-0 flex-wrap">
        {FILTERS.map((f) => (
          <button
            key={f.id}
            data-testid={`history-filter-${f.id}`}
            onClick={() => { setFilter(f.id); setNotesInitialId(null); }}
            className={[
              "px-3 py-1 rounded-full text-[10px] font-medium transition-all border",
              filter === f.id && !q
                ? "bg-[rgba(245,158,11,0.15)] border-[rgba(245,158,11,0.3)] text-[#fbbf24]"
                : "bg-[rgba(255,255,255,0.04)] border-transparent text-[rgba(255,255,255,0.35)] hover:text-[rgba(255,255,255,0.55)]",
            ].join(" ")}
          >
            {t(f.label)}
          </button>
        ))}
      </div>

      {/* Content */}
      {q ? (
        <SearchResults
          query={q}
          results={results}
          searching={searching}
          searched={searched}
          error={error}
          expandedId={expandedId}
          setExpandedId={setExpandedId}
          containers={containers}
          onOpenNotebook={openNotebookOf}
          onOpenMeeting={openMeetingOf}
          onCopy={handleCopy}
          onDelete={handleDelete}
        />
      ) : filter === "note" ? (
        <div className="flex-1 overflow-hidden">
          <Notes key={`notes-${notesRefresh}`} embedded initialNotebookId={notesInitialId ?? undefined} />
        </div>
      ) : filter === "meeting" ? (
        <div className="flex-1 overflow-hidden">
          <Meetings embedded onOpenDetail={(c) => setOpenMeeting(c)} />
        </div>
      ) : (
        <>
          <div className="flex-1 overflow-y-auto px-5 pb-4">
            {error ? (
              <div className="text-[rgba(239,68,68,0.7)] text-[12px] py-8 text-center">{t("history.error")} {error}</div>
            ) : loading ? (
              <div className="text-[rgba(255,255,255,0.2)] text-[12px] py-10 text-center">{t("history.loading")}</div>
            ) : timeline.length === 0 ? (
              <div className="text-[rgba(255,255,255,0.2)] text-[12px] py-10 text-center">
                {filter === "all"
                  ? t("history.empty-all")
                  : t("history.empty-filter").replace("{filter}", t(FILTERS.find((f) => f.id === filter)?.label ?? "history.filter.all"))}
              </div>
            ) : (
              <div className="flex flex-col gap-1.5">
                {timeline.map((item) =>
                  item.kind === "meeting" ? (
                    <MeetingCard
                      key={`m-${item.container?.id ?? item.latest.id}`}
                      item={item}
                      onOpen={() => item.container && setOpenMeeting(item.container)}
                    />
                  ) : (
                    <EntryCard
                      key={item.entry.id}
                      entry={item.entry}
                      containers={containers}
                      expanded={expandedId === item.entry.id}
                      onToggle={() => setExpandedId(expandedId === item.entry.id ? null : item.entry.id)}
                      onOpenNotebook={() => openNotebookOf(item.entry)}
                      onCopy={() => handleCopy(entryText(item.entry))}
                      onDelete={() => handleDelete(item.entry.id)}
                      onCorrect={openCorrectMenu}
                    />
                  )
                )}
              </div>
            )}
          </div>

          {/* Pagination */}
          {(page > 0 || entries.length === PAGE_SIZE) && (
            <div className="flex justify-center items-center gap-4 px-5 py-2.5 border-t border-[rgba(255,255,255,0.04)] flex-shrink-0">
              <button
                onClick={() => { const p = page - 1; setPage(p); loadTimeline(p, filter); }}
                disabled={page === 0}
                className="text-[11px] disabled:text-[rgba(255,255,255,0.12)] text-[rgba(255,255,255,0.4)] hover:text-[rgba(255,255,255,0.7)]"
              >
                {t("history.prev")}
              </button>
              <span className="text-[11px] text-[rgba(255,255,255,0.25)]">{t("history.page").replace("{n}", String(page + 1))}</span>
              <button
                onClick={() => { const p = page + 1; setPage(p); loadTimeline(p, filter); }}
                disabled={entries.length < PAGE_SIZE}
                className="text-[11px] disabled:text-[rgba(255,255,255,0.12)] text-[rgba(255,255,255,0.4)] hover:text-[rgba(255,255,255,0.7)]"
              >
                {t("history.next")}
              </button>
            </div>
          )}
        </>
      )}

      {/* Correction capture (issue #31): floating menu + anchored popover */}
      {menu && (
        <CorrectionMenu
          anchor={menu}
          onCorrect={() => { setPopover(menu); setMenu(null); }}
          onCopy={() => { handleCopy(menu.selection); setMenu(null); }}
          onClose={() => setMenu(null)}
        />
      )}
      {popover && (
        <CorrectionPopover
          anchor={popover}
          onClose={() => setPopover(null)}
          onApplied={applyCorrection}
        />
      )}
    </div>
  );
}

// ─── Cards ────────────────────────────────────────────────────────────────────

function TypeBadge({ type }: { type: string }) {
  const c = STRIPE_COLOR[type] || "#a8a29e";
  return (
    <span
      className="text-[8px] font-semibold px-1.5 py-0.5 rounded uppercase tracking-wide"
      style={{ background: `${c}18`, color: c }}
    >
      {type}
    </span>
  );
}

/** Dictation / agent / note entry — compact card, expand in place. */
function EntryCard({
  entry,
  containers,
  expanded,
  onToggle,
  onOpenNotebook,
  onCopy,
  onDelete,
  onCorrect,
}: {
  entry: Entry;
  containers: Map<number, Container>;
  expanded: boolean;
  onToggle: () => void;
  onOpenNotebook: () => void;
  onCopy: () => void;
  onDelete: () => void;
  onCorrect?: (entry: Entry, selection: string, rect: DOMRect) => void;
}) {
  const stripe = STRIPE_COLOR[entry.source_type] || "#a8a29e";
  const text = entryText(entry);
  const notebook = entry.source_type === "note" && entry.container_id != null
    ? containers.get(entry.container_id)
    : null;
  const listenTitle = entry.source_type === "listen"
    ? String((entry.metadata as Record<string, unknown>)?.title ?? "")
    : "";

  // Open the correction menu when the user selects text inside this card's
  // transcript. Uses the native browser selection — we never fight it.
  const emitSelection = (e: ReactMouseEvent<HTMLElement>) => {
    if (!onCorrect) return;
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed) return;
    const selText = sel.toString().trim();
    if (!selText) return;
    const node = sel.anchorNode;
    if (node && !e.currentTarget.contains(node)) return;
    const rect = sel.getRangeAt(0).getBoundingClientRect();
    e.stopPropagation();
    onCorrect(entry, selText, rect);
  };

  return (
    <div
      data-testid="history-card"
      onClick={(e) => {
        // A drag-selection ends in a click — don't toggle while text is selected.
        if (window.getSelection()?.toString().trim()) { e.preventDefault(); return; }
        onToggle();
      }}
      className={[
        "flex rounded-[10px] border overflow-hidden cursor-pointer transition-colors",
        expanded
          ? "border-[rgba(245,158,11,0.25)] bg-[rgba(255,255,255,0.04)]"
          : "border-[rgba(255,255,255,0.06)] bg-[rgba(255,255,255,0.025)] hover:bg-[rgba(255,255,255,0.035)]",
      ].join(" ")}
    >
      <div className="w-[3px] flex-shrink-0" style={{ background: stripe }} />
      <div className="flex-1 px-3 py-2 min-w-0">
        <div className="flex items-center gap-1.5 mb-1">
          <span className="text-[9px] text-[rgba(255,255,255,0.25)] font-mono">{formatTime(entry.created_at)}</span>
          <TypeBadge type={entry.source_type} />
          {entry.mode && entry.mode !== "raw" && entry.mode !== entry.source_type && (
            <span className="text-[8px] text-[rgba(255,255,255,0.18)] bg-[rgba(255,255,255,0.03)] px-1.5 py-0.5 rounded">{entry.mode}</span>
          )}
          {notebook && (
            <button
              onClick={(e) => { e.stopPropagation(); onOpenNotebook(); }}
              className="text-[8px] px-1.5 py-0.5 rounded bg-[rgba(74,222,128,0.08)] text-[rgba(74,222,128,0.7)] hover:bg-[rgba(74,222,128,0.15)] transition-colors"
            >
              {notebook.title} →
            </button>
          )}
          {listenTitle && (
            <span className="text-[11px] font-medium text-[#fafaf9] truncate flex-1">{listenTitle}</span>
          )}
        </div>
        {expanded ? (
          <>
            <div
              onMouseUp={emitSelection}
              className="text-[11px] leading-relaxed text-[rgba(255,255,255,0.7)] whitespace-pre-wrap break-words select-text cursor-text"
            >
              {text || <span className="italic text-[rgba(255,255,255,0.2)]">{t("history.no-content")}</span>}
            </div>
            <div className="flex gap-2 mt-2" onClick={(e) => e.stopPropagation()}>
              {entry.source_type === "listen" && entry.audio_ref && (
                <>
                  <button
                    onClick={() => { playAudioFile(entry.audio_ref!).catch(() => {}); }}
                    className="text-[9px] px-2.5 py-1 rounded-md bg-[rgba(125,211,252,0.12)] text-[#7dd3fc] hover:bg-[rgba(125,211,252,0.2)] transition-colors"
                  >
                    {t("common.play")}
                  </button>
                  <button
                    onClick={() => { stopPlayback().catch(() => {}); }}
                    className="text-[9px] px-2 py-1 rounded-md bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)] transition-colors"
                  >
                    {t("common.stop")}
                  </button>
                </>
              )}
              <button
                onClick={onCopy}
                className="text-[9px] px-2 py-1 rounded-md bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)] transition-colors"
              >
                {t("common.copy")}
              </button>
              <button
                onClick={onDelete}
                className="text-[9px] px-2 py-1 rounded-md bg-[rgba(239,68,68,0.06)] text-[rgba(239,68,68,0.5)] hover:text-[#ef4444] transition-colors"
              >
                {t("common.delete")}
              </button>
            </div>
          </>
        ) : (
          <div
            onMouseUp={emitSelection}
            className={["text-[11px] leading-normal break-words select-text cursor-text", text ? "text-[rgba(255,255,255,0.55)]" : "italic text-[rgba(255,255,255,0.2)]"].join(" ")}
          >
            {preview(text) || t("history.no-content")}
          </div>
        )}
      </div>
    </div>
  );
}

/** Meeting session — one card per meeting, summary-forward. */
function MeetingCard({
  item,
  onOpen,
}: {
  item: Extract<TimelineItem, { kind: "meeting" }>;
  onOpen: () => void;
}) {
  const c = item.container;
  const meta = (c?.metadata ?? {}) as Record<string, unknown>;
  const summary = typeof meta.summary_preview === "string" ? meta.summary_preview : "";
  return (
    <div
      data-testid="history-meeting-card"
      onClick={onOpen}
      className="flex rounded-[10px] border border-[rgba(251,191,36,0.12)] bg-[rgba(251,191,36,0.03)] overflow-hidden cursor-pointer hover:bg-[rgba(251,191,36,0.06)] transition-colors"
    >
      <div className="w-[3px] flex-shrink-0 bg-[#fbbf24]" />
      <div className="flex-1 px-3 py-2.5 min-w-0">
        <div className="flex items-center gap-1.5 mb-1">
          <span className="text-[9px] text-[rgba(255,255,255,0.25)] font-mono">{formatTime(item.latest.created_at)}</span>
          <TypeBadge type="meeting" />
          <span className="text-[11px] font-medium text-[#fafaf9] truncate flex-1">
            {c?.title || t("tab.meeting")}
          </span>
          <span className="text-[9px] text-[rgba(255,255,255,0.25)] flex-shrink-0">
            {item.segments} {item.segments === 1 ? t("meet.segment") : t("meet.segments")} ›
          </span>
        </div>
        <div className="text-[11px] leading-normal text-[rgba(255,255,255,0.5)] break-words">
          {summary ? preview(summary, 180) : preview(entryText(item.latest))}
        </div>
      </div>
    </div>
  );
}

// ─── Search results ───────────────────────────────────────────────────────────

const GROUP_ORDER: SourceType[] = ["dictation", "note", "meeting", "listen", "agent"];
const GROUP_LABEL: Record<SourceType, TKey> = {
  dictation: "history.group.dictation",
  note: "history.filter.notes",
  meeting: "history.filter.meetings",
  listen: "history.filter.listen",
  agent: "history.filter.agent",
};

function SearchResults({
  query,
  results,
  searching,
  searched,
  error,
  expandedId,
  setExpandedId,
  containers,
  onOpenNotebook,
  onOpenMeeting,
  onCopy,
  onDelete,
}: {
  query: string;
  results: Entry[];
  searching: boolean;
  searched: boolean;
  error: string;
  expandedId: number | null;
  setExpandedId: (id: number | null) => void;
  containers: Map<number, Container>;
  onOpenNotebook: (e: Entry) => void;
  onOpenMeeting: (e: Entry) => void;
  onCopy: (text: string) => void;
  onDelete: (id: number) => void;
}) {
  const groups = GROUP_ORDER
    .map((type) => ({ type, items: results.filter((r) => r.source_type === type) }))
    .filter((g) => g.items.length > 0);

  return (
    <div className="flex-1 overflow-y-auto px-5 pb-4">
      {error ? (
        <div className="text-[rgba(239,68,68,0.7)] text-[12px] py-8 text-center">{t("history.search-failed")} {error}</div>
      ) : searching ? (
        <div className="text-[rgba(255,255,255,0.2)] text-[12px] py-10 text-center">{t("history.searching")}</div>
      ) : searched && results.length === 0 ? (
        <div className="text-[rgba(255,255,255,0.2)] text-[12px] py-10 text-center">{t("history.no-matches")} “{query}”</div>
      ) : (
        <div className="flex flex-col gap-3.5">
          {groups.map((group) => (
            <div key={group.type}>
              <div
                className="text-[9px] font-semibold uppercase tracking-wider mb-1.5 flex items-center gap-1.5"
                style={{ color: STRIPE_COLOR[group.type] }}
              >
                {t(GROUP_LABEL[group.type])}
                <span className="text-[rgba(255,255,255,0.2)] font-normal">{group.items.length}</span>
              </div>
              <div className="flex flex-col gap-1.5">
                {group.items.map((entry) => {
                  const stripe = STRIPE_COLOR[entry.source_type] || "#a8a29e";
                  const expanded = expandedId === entry.id;
                  const text = entryText(entry);
                  const snip = makeSnippet(text, query);
                  const canOpenNotebook = entry.source_type === "note" && entry.container_id != null;
                  const canOpenMeeting = entry.source_type === "meeting" && entry.container_id != null && containers.has(entry.container_id);
                  return (
                    <div
                      key={entry.id}
                      data-testid="history-search-result"
                      onClick={() => setExpandedId(expanded ? null : entry.id)}
                      className={[
                        "flex rounded-[10px] border overflow-hidden cursor-pointer transition-colors",
                        expanded
                          ? "border-[rgba(245,158,11,0.25)] bg-[rgba(255,255,255,0.04)]"
                          : "border-[rgba(255,255,255,0.06)] bg-[rgba(255,255,255,0.025)] hover:bg-[rgba(255,255,255,0.035)]",
                      ].join(" ")}
                    >
                      <div className="w-[3px] flex-shrink-0" style={{ background: stripe }} />
                      <div className="flex-1 px-3 py-2 min-w-0">
                        <div className="flex items-center gap-1.5 mb-1">
                          <span className="text-[9px] text-[rgba(255,255,255,0.25)] font-mono">{formatTime(entry.created_at)}</span>
                          <TypeBadge type={entry.source_type} />
                        </div>
                        {expanded ? (
                          <>
                            <div className="text-[11px] leading-relaxed text-[rgba(255,255,255,0.7)] whitespace-pre-wrap break-words">{text}</div>
                            <div className="flex gap-2 mt-2" onClick={(e) => e.stopPropagation()}>
                              {canOpenNotebook && (
                                <button onClick={() => onOpenNotebook(entry)} className="text-[9px] px-2 py-1 rounded-md bg-[rgba(74,222,128,0.08)] text-[rgba(74,222,128,0.7)] hover:bg-[rgba(74,222,128,0.15)] transition-colors">
                                  {t("history.open-notebook")}
                                </button>
                              )}
                              {canOpenMeeting && (
                                <button onClick={() => onOpenMeeting(entry)} className="text-[9px] px-2 py-1 rounded-md bg-[rgba(251,191,36,0.08)] text-[rgba(251,191,36,0.7)] hover:bg-[rgba(251,191,36,0.15)] transition-colors">
                                  {t("history.open-meeting")}
                                </button>
                              )}
                              <button onClick={() => onCopy(text)} className="text-[9px] px-2 py-1 rounded-md bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)] transition-colors">
                                {t("common.copy")}
                              </button>
                              <button onClick={() => onDelete(entry.id)} className="text-[9px] px-2 py-1 rounded-md bg-[rgba(239,68,68,0.06)] text-[rgba(239,68,68,0.5)] hover:text-[#ef4444] transition-colors">
                                {t("common.delete")}
                              </button>
                            </div>
                          </>
                        ) : (
                          <div className="text-[11px] leading-normal text-[rgba(255,255,255,0.55)] break-words">
                            {snip.before}
                            {snip.match && (
                              <span className="rounded-[2px] px-[1px]" style={{ background: "rgba(245,158,11,0.22)", color: "#fbbf24" }}>
                                {snip.match}
                              </span>
                            )}
                            {snip.after}
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ─── Correction capture (issue #31) ─────────────────────────────────────────────

/** A pending correction: which entry, the selected mis-transcribed text, and
 *  the on-screen rect of the selection to anchor the menu / popover to. */
type CorrectionAnchor = { entry: Entry; selection: string; rect: DOMRect };

const LAST_BOOK_KEY = "fonos.correct.lastBook";

function truncateSel(s: string, max = 24): string {
  return s.length > max ? s.slice(0, max) + "…" : s;
}

/** Position a floating layer just below the selection, clamped to the viewport
 *  (flips above when it would overflow the bottom edge). */
function clampToViewport(rect: DOMRect, w: number, h: number): { left: number; top: number } {
  const margin = 8;
  let left = rect.left;
  let top = rect.bottom + 6;
  if (left + w > window.innerWidth - margin) left = window.innerWidth - margin - w;
  if (left < margin) left = margin;
  if (top + h > window.innerHeight - margin) {
    const above = rect.top - h - 6;
    top = above > margin ? above : margin;
  }
  return { left, top };
}

/** Small radio dot matching the vocab book-picker rows. */
function Radio({ on }: { on: boolean }) {
  return (
    <span
      className={[
        "flex h-[13px] w-[13px] shrink-0 items-center justify-center rounded-full border-[1.5px]",
        on ? "border-[#fbbf24]" : "border-[rgba(255,255,255,0.2)]",
      ].join(" ")}
    >
      {on && <span className="h-1.5 w-1.5 rounded-full bg-[#fbbf24]" />}
    </span>
  );
}

/** The "✎ Correct …" floating context menu shown next to the selection. */
function CorrectionMenu({
  anchor,
  onCorrect,
  onCopy,
  onClose,
}: {
  anchor: CorrectionAnchor;
  onCorrect: () => void;
  onCopy: () => void;
  onClose: () => void;
}) {
  useT();
  const pos = clampToViewport(anchor.rect, 210, 92);
  return (
    <>
      <div className="fixed inset-0 z-40" onMouseDown={onClose} />
      <div
        className="fixed z-50 w-[210px] overflow-hidden rounded-[10px] border border-[rgba(255,255,255,0.1)] bg-[#242220] text-[11.5px] shadow-[0_16px_40px_rgba(0,0,0,0.55)]"
        style={{ left: pos.left, top: pos.top }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <button
          onClick={onCorrect}
          className="flex w-full items-center gap-2.5 px-3.5 py-2 text-left font-semibold text-[#fbbf24] bg-[rgba(251,191,36,0.1)] hover:bg-[rgba(251,191,36,0.16)] transition-colors"
        >
          <span className="shrink-0">✎</span>
          <span className="truncate">{t("correct.menu").replace("{sel}", truncateSel(anchor.selection))}</span>
        </button>
        <div className="border-t border-[rgba(255,255,255,0.06)]" />
        <button
          onClick={onCopy}
          className="flex w-full items-center gap-2.5 px-3.5 py-2 text-left text-[rgba(255,255,255,0.55)] hover:bg-[rgba(255,255,255,0.04)] transition-colors"
        >
          {t("common.copy")}
        </button>
      </div>
    </>
  );
}

/** The anchored correction popover: change-to input, live rule preview,
 *  rule/term choice, vocab-book picker, and "Save & fix this entry". */
function CorrectionPopover({
  anchor,
  onClose,
  onApplied,
}: {
  anchor: CorrectionAnchor;
  onClose: () => void;
  onApplied: (entryId: number, newText: string) => void;
}) {
  useT();
  const { entry, selection } = anchor;
  const [replacement, setReplacement] = useState("");
  const [asRule, setAsRule] = useState(true);
  const [books, setBooks] = useState<VocabBook[]>([]);
  const [globalIds, setGlobalIds] = useState<string[]>([]);
  const [selectedBookId, setSelectedBookId] = useState<string | null>(null);
  const [newBookMode, setNewBookMode] = useState(false);
  const [newBookName, setNewBookName] = useState("");
  const [saving, setSaving] = useState(false);
  const changeToRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    changeToRef.current?.focus();
  }, []);

  // History carries no config of its own — load it lazily to read/write books.
  useEffect(() => {
    let cancelled = false;
    getConfig()
      .then((cfg: AppConfig) => {
        if (cancelled) return;
        const bks = cfg.vocab_books ?? [];
        setBooks(bks);
        setGlobalIds(cfg.global_vocab_books ?? []);
        let last: string | null = null;
        try { last = localStorage.getItem(LAST_BOOK_KEY); } catch { /* ignore */ }
        const initial = bks.find((b) => b.id === last) ?? bks[0] ?? null;
        if (initial) setSelectedBookId(initial.id);
        else setNewBookMode(true);
      })
      .catch(() => { if (!cancelled) setNewBookMode(true); });
    return () => { cancelled = true; };
  }, []);

  const to = replacement.trim();
  const canSave =
    to.length > 0 &&
    (newBookMode ? newBookName.trim().length > 0 : selectedBookId != null) &&
    !saving;

  const handleSave = async () => {
    if (!canSave) return;
    setSaving(true);
    try {
      let nextBooks = books.map((b) => ({ ...b }));
      const nextGlobal = [...globalIds];
      let targetId: string;

      if (newBookMode) {
        const book: VocabBook = {
          id: `book-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`,
          name: newBookName.trim(),
          enabled: true,
          terms: [],
          rules: [],
        };
        nextBooks.push(book);
        targetId = book.id;
        // A freshly created book is attached to global so it is live immediately.
        if (!nextGlobal.includes(book.id)) nextGlobal.push(book.id);
      } else {
        targetId = selectedBookId as string;
      }

      nextBooks = nextBooks.map((b) => {
        if (b.id !== targetId) return b;
        if (asRule) {
          const rule: VocabRule = { from: selection, to, kind: "literal", case_insensitive: true };
          return { ...b, rules: [...b.rules, rule] };
        }
        const terms = b.terms.includes(to) ? b.terms : [...b.terms, to];
        return { ...b, terms };
      });

      await saveConfig(JSON.stringify({ vocab_books: nextBooks, global_vocab_books: nextGlobal }));

      // Fix this entry: replace every occurrence of the selection in the shown text.
      const shown = entry.processed_text || entry.raw_text || "";
      const corrected = shown.split(selection).join(to);
      await updateEntryText(entry.id, corrected);
      onApplied(entry.id, corrected);

      try { localStorage.setItem(LAST_BOOK_KEY, targetId); } catch { /* ignore */ }
      onClose();
    } catch {
      setSaving(false); // keep the popover open so the user can retry
    }
  };

  const pos = clampToViewport(anchor.rect, 320, 360);
  const segClass = (on: boolean) =>
    [
      "flex-1 py-1.5 text-center text-[10.5px] transition-colors",
      on ? "bg-[rgba(251,191,36,0.13)] text-[#fbbf24] font-semibold" : "text-[rgba(255,255,255,0.32)] hover:text-[rgba(255,255,255,0.55)]",
    ].join(" ");

  return (
    <>
      <div className="fixed inset-0 z-40" onMouseDown={onClose} />
      <div
        className="fixed z-50 flex w-[320px] max-h-[calc(100vh-16px)] flex-col overflow-y-auto rounded-[14px] border border-[rgba(255,255,255,0.1)] bg-[#232120] p-4 shadow-[0_20px_50px_rgba(0,0,0,0.55)]"
        style={{ left: pos.left, top: pos.top }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="mb-2.5 text-[12px] font-semibold text-[#fafaf9]">{t("correct.title")}</div>

        {/* Change to */}
        <div className="flex items-center gap-2">
          <span className="shrink-0 text-[10.5px] text-[rgba(255,255,255,0.32)]">{t("correct.changeto")}</span>
          <input
            ref={changeToRef}
            type="text"
            value={replacement}
            onChange={(e) => setReplacement(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleSave(); }}
            placeholder={t("correct.changeto-ph")}
            spellCheck={false}
            className="flex-1 rounded-lg border border-[rgba(255,255,255,0.07)] bg-[rgba(255,255,255,0.04)] px-2.5 py-1.5 text-[11.5px] text-[#fafaf9] outline-none focus:border-[rgba(251,191,36,0.3)] placeholder:text-[rgba(255,255,255,0.25)]"
          />
        </div>

        {/* Live preview: strikethrough-red → green (from-part hidden for term-only) */}
        <div className="my-3 flex items-center gap-2.5 rounded-[9px] border border-[rgba(251,191,36,0.15)] bg-[rgba(251,191,36,0.05)] px-3 py-2 text-[12px]">
          {asRule && (
            <>
              <span className="truncate text-[#f87171] line-through decoration-[rgba(248,113,113,0.5)]">{selection}</span>
              <span className="flex h-[18px] w-[18px] shrink-0 items-center justify-center rounded-full bg-[rgba(251,191,36,0.12)] text-[10px] text-[#fbbf24]">→</span>
            </>
          )}
          {to ? (
            <span className="truncate font-semibold text-[#4ade80]">{to}</span>
          ) : (
            <span className="text-[rgba(74,222,128,0.4)]">…</span>
          )}
          <span className="ml-auto shrink-0 text-[10.5px] text-[rgba(255,255,255,0.32)]">{t("correct.will-fix")}</span>
        </div>

        {/* Rule vs term */}
        <div className="mb-3 flex overflow-hidden rounded-lg border border-[rgba(255,255,255,0.07)]">
          <button onClick={() => setAsRule(true)} className={segClass(asRule)}>{t("correct.asrule")}</button>
          <button onClick={() => setAsRule(false)} className={segClass(!asRule)}>{t("correct.asterm")}</button>
        </div>

        {/* Book picker */}
        <div className="flex max-h-[168px] flex-col gap-1.5 overflow-y-auto">
          {books.map((book) => {
            const on = !newBookMode && selectedBookId === book.id;
            const isGlobal = globalIds.includes(book.id);
            return (
              <button
                key={book.id}
                onClick={() => { setNewBookMode(false); setSelectedBookId(book.id); }}
                className={[
                  "flex items-center gap-2.5 rounded-[9px] border px-3 py-2 text-left text-[11px] transition-colors",
                  on
                    ? "border-[rgba(251,191,36,0.4)] bg-[rgba(251,191,36,0.05)] text-[#fafaf9]"
                    : "border-[rgba(255,255,255,0.07)] text-[rgba(255,255,255,0.55)] hover:border-[rgba(255,255,255,0.12)]",
                ].join(" ")}
              >
                <Radio on={on} />
                <span className="truncate">{book.name || t("vocab.unnamed")}</span>
                {isGlobal && (
                  <span className="shrink-0 rounded-full bg-[rgba(245,158,11,0.12)] px-1.5 py-0.5 text-[8px] font-semibold uppercase tracking-wide text-[#fbbf24]">
                    {t("common.global")}
                  </span>
                )}
                <span className="ml-auto shrink-0 text-[9.5px] text-[rgba(255,255,255,0.32)]">
                  {book.terms.length} {t("vocab.termcount")} · {book.rules.length} {t("vocab.rulecount")}
                </span>
              </button>
            );
          })}

          {newBookMode ? (
            <div className="flex items-center gap-2.5 rounded-[9px] border border-[rgba(251,191,36,0.4)] bg-[rgba(251,191,36,0.05)] px-3 py-2">
              <Radio on={true} />
              <input
                autoFocus
                type="text"
                value={newBookName}
                onChange={(e) => setNewBookName(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") handleSave(); }}
                placeholder={t("correct.bookname")}
                spellCheck={false}
                className="flex-1 bg-transparent text-[11px] text-[#fafaf9] outline-none placeholder:text-[rgba(255,255,255,0.25)]"
              />
              {books.length > 0 && (
                <button
                  onClick={() => { setNewBookMode(false); setSelectedBookId(books[0].id); }}
                  className="shrink-0 text-[12px] leading-none text-[rgba(255,255,255,0.3)] hover:text-[rgba(255,255,255,0.6)]"
                >
                  ×
                </button>
              )}
            </div>
          ) : (
            <button
              onClick={() => setNewBookMode(true)}
              className="flex items-center gap-2.5 rounded-[9px] border border-[rgba(255,255,255,0.07)] px-3 py-2 text-left text-[11px] text-[rgba(255,255,255,0.45)] hover:border-[rgba(251,191,36,0.3)] hover:text-[#fbbf24] transition-colors"
            >
              <span className="w-[13px] shrink-0" />
              {t("correct.newbook")}
            </button>
          )}
        </div>

        {/* Save */}
        <div className="mt-3.5 flex items-center gap-2">
          <button
            onClick={handleSave}
            disabled={!canSave}
            className="shrink-0 rounded-lg border border-[rgba(251,191,36,0.35)] bg-[rgba(251,191,36,0.14)] px-3.5 py-1.5 text-[11px] font-semibold text-[#fbbf24] transition-colors hover:bg-[rgba(251,191,36,0.2)] disabled:opacity-40 disabled:hover:bg-[rgba(251,191,36,0.14)]"
          >
            {t("correct.save")}
          </button>
          <span className="text-[10.5px] text-[rgba(255,255,255,0.32)]">{t("correct.savenote")}</span>
        </div>
      </div>
    </>
  );
}
