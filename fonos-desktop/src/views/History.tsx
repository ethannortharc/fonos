// History — unified browsing over everything captured (issue: menu consolidation).
// One entry point replaces Recent / Search / Notes / Meetings tabs:
//   search box (FTS) + type filter chips + type-appropriate content:
//     all|dictation|agent → unified timeline (meetings collapsed to one card)
//     note               → embedded Notes browser (notebook tabs + entries)
//     meeting            → embedded Meetings list; detail via stack push

import { useState, useEffect, useCallback, useMemo } from "react";
import { listEntries, listContainers, searchEntries, deleteEntry } from "../lib/storage-api";
import type { Entry, Container, SourceType } from "../lib/storage-api";
import Notes from "./Notes";
import Meetings from "./Meetings";
import { MeetingDetailView } from "./Meetings";

const PAGE_SIZE = 30;
const SEARCH_DEBOUNCE_MS = 250;

export type HistoryFilter = "all" | SourceType;

const FILTERS: { id: HistoryFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "dictation", label: "Dictation" },
  { id: "note", label: "Notes" },
  { id: "meeting", label: "Meetings" },
  { id: "agent", label: "Agent" },
];

const STRIPE_COLOR: Record<string, string> = {
  dictation: "#a8a29e",
  agent: "#c084fc",
  note: "#4ade80",
  meeting: "#fbbf24",
};

function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    const now = new Date();
    const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    if (d.toDateString() === now.toDateString()) return `Today ${time}`;
    const y = new Date(now.getTime() - 86400000);
    if (d.toDateString() === y.toDateString()) return `Yesterday ${time}`;
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

  const timelineActive = !query.trim() && (filter === "all" || filter === "dictation" || filter === "agent");

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
        <h2 className="text-[13px] font-semibold text-[#fafaf9] flex-shrink-0">History</h2>
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
            placeholder="Search dictations, notes, meetings…"
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
            {f.label}
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
              <div className="text-[rgba(239,68,68,0.7)] text-[12px] py-8 text-center">Error: {error}</div>
            ) : loading ? (
              <div className="text-[rgba(255,255,255,0.2)] text-[12px] py-10 text-center">Loading…</div>
            ) : timeline.length === 0 ? (
              <div className="text-[rgba(255,255,255,0.2)] text-[12px] py-10 text-center">
                {filter === "all" ? "Nothing captured yet" : `No ${filter} entries`}
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
                ← Previous
              </button>
              <span className="text-[11px] text-[rgba(255,255,255,0.25)]">Page {page + 1}</span>
              <button
                onClick={() => { const p = page + 1; setPage(p); loadTimeline(p, filter); }}
                disabled={entries.length < PAGE_SIZE}
                className="text-[11px] disabled:text-[rgba(255,255,255,0.12)] text-[rgba(255,255,255,0.4)] hover:text-[rgba(255,255,255,0.7)]"
              >
                Next →
              </button>
            </div>
          )}
        </>
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
}: {
  entry: Entry;
  containers: Map<number, Container>;
  expanded: boolean;
  onToggle: () => void;
  onOpenNotebook: () => void;
  onCopy: () => void;
  onDelete: () => void;
}) {
  const stripe = STRIPE_COLOR[entry.source_type] || "#a8a29e";
  const text = entryText(entry);
  const notebook = entry.source_type === "note" && entry.container_id != null
    ? containers.get(entry.container_id)
    : null;

  return (
    <div
      data-testid="history-card"
      onClick={onToggle}
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
        </div>
        {expanded ? (
          <>
            <div className="text-[11px] leading-relaxed text-[rgba(255,255,255,0.7)] whitespace-pre-wrap break-words">
              {text || <span className="italic text-[rgba(255,255,255,0.2)]">(no content)</span>}
            </div>
            <div className="flex gap-2 mt-2" onClick={(e) => e.stopPropagation()}>
              <button
                onClick={onCopy}
                className="text-[9px] px-2 py-1 rounded-md bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)] transition-colors"
              >
                Copy
              </button>
              <button
                onClick={onDelete}
                className="text-[9px] px-2 py-1 rounded-md bg-[rgba(239,68,68,0.06)] text-[rgba(239,68,68,0.5)] hover:text-[#ef4444] transition-colors"
              >
                Delete
              </button>
            </div>
          </>
        ) : (
          <div className={["text-[11px] leading-normal break-words", text ? "text-[rgba(255,255,255,0.55)]" : "italic text-[rgba(255,255,255,0.2)]"].join(" ")}>
            {preview(text) || "(no content)"}
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
            {c?.title || "Meeting"}
          </span>
          <span className="text-[9px] text-[rgba(255,255,255,0.25)] flex-shrink-0">
            {item.segments} segment{item.segments === 1 ? "" : "s"} ›
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

const GROUP_ORDER: SourceType[] = ["dictation", "note", "meeting", "agent"];
const GROUP_LABEL: Record<SourceType, string> = {
  dictation: "Dictations",
  note: "Notes",
  meeting: "Meetings",
  agent: "Agent",
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
        <div className="text-[rgba(239,68,68,0.7)] text-[12px] py-8 text-center">Search failed: {error}</div>
      ) : searching ? (
        <div className="text-[rgba(255,255,255,0.2)] text-[12px] py-10 text-center">Searching…</div>
      ) : searched && results.length === 0 ? (
        <div className="text-[rgba(255,255,255,0.2)] text-[12px] py-10 text-center">No matches for “{query}”</div>
      ) : (
        <div className="flex flex-col gap-3.5">
          {groups.map((group) => (
            <div key={group.type}>
              <div
                className="text-[9px] font-semibold uppercase tracking-wider mb-1.5 flex items-center gap-1.5"
                style={{ color: STRIPE_COLOR[group.type] }}
              >
                {GROUP_LABEL[group.type]}
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
                                  Open notebook →
                                </button>
                              )}
                              {canOpenMeeting && (
                                <button onClick={() => onOpenMeeting(entry)} className="text-[9px] px-2 py-1 rounded-md bg-[rgba(251,191,36,0.08)] text-[rgba(251,191,36,0.7)] hover:bg-[rgba(251,191,36,0.15)] transition-colors">
                                  Open meeting →
                                </button>
                              )}
                              <button onClick={() => onCopy(text)} className="text-[9px] px-2 py-1 rounded-md bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)] transition-colors">
                                Copy
                              </button>
                              <button onClick={() => onDelete(entry.id)} className="text-[9px] px-2 py-1 rounded-md bg-[rgba(239,68,68,0.06)] text-[rgba(239,68,68,0.5)] hover:text-[#ef4444] transition-colors">
                                Delete
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
