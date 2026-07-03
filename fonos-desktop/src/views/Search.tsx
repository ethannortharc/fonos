// Search view — global full-text search over all entries (issue #9).
// Uses inline styles to avoid Tailwind v4 utility generation issues (same as Recent).

import { useState, useEffect, useRef } from "react";
import { searchEntries } from "../lib/storage-api";
import type { Entry, SourceType } from "../lib/storage-api";

const DEBOUNCE_MS = 250;
const RESULT_LIMIT = 50;

const STRIPE_COLOR: Record<string, string> = {
  dictation: "#a8a29e",
  agent: "#c084fc",
  note: "#4ade80",
  meeting: "#fbbf24",
};

const GROUP_ORDER: SourceType[] = ["dictation", "note", "meeting", "agent"];

const GROUP_LABEL: Record<SourceType, string> = {
  dictation: "Dictations",
  note: "Notes",
  meeting: "Meetings",
  agent: "Agent",
};

/** Tab of the parent container view for entry types that have one. */
const CONTAINER_TAB: Partial<Record<SourceType, "notes" | "meetings">> = {
  note: "notes",
  meeting: "meetings",
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
  } catch { return iso; }
}

interface Snippet {
  before: string;
  match: string;
  after: string;
}

/** Build a snippet centred on the first occurrence of the query (or its first
 *  longish token), so the user sees WHY the entry matched. */
function makeSnippet(text: string, query: string): Snippet {
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
  // Tokens matched in different places (FTS AND) — fall back to a plain preview.
  return {
    before: "",
    match: "",
    after: text.length > 150 ? text.slice(0, 150) + "…" : text,
  };
}

function entryText(e: Entry): string {
  return e.processed_text || e.raw_text || "";
}

interface SearchProps {
  onNavigate: (tab: "notes" | "meetings") => void;
}

export default function Search({ onNavigate }: SearchProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Entry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [searched, setSearched] = useState(false);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Debounced search-as-you-type
  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setResults([]);
      setSearched(false);
      setError("");
      setLoading(false);
      return;
    }
    setLoading(true);
    // `cancelled` guards against out-of-order responses: a slow request for a
    // previous query must not overwrite results of the current one.
    let cancelled = false;
    const timer = setTimeout(async () => {
      try {
        const found = await searchEntries(q, RESULT_LIMIT);
        if (cancelled) return;
        setResults(found);
        setError("");
      } catch (err) {
        if (cancelled) return;
        console.error("searchEntries error:", err);
        setError(String(err));
        setResults([]);
      } finally {
        if (!cancelled) {
          setLoading(false);
          setSearched(true);
          setExpandedId(null);
        }
      }
    }, DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query]);

  const groups = GROUP_ORDER
    .map((type) => ({ type, items: results.filter((r) => r.source_type === type) }))
    .filter((g) => g.items.length > 0);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", background: "#1a1917" }}>
      {/* Header */}
      <div style={{ padding: "16px 20px 8px", flexShrink: 0 }}>
        <h2 style={{ fontSize: 13, fontWeight: 600, color: "#fafaf9", margin: 0 }}>Search</h2>
      </div>

      {/* Search box */}
      <div style={{ padding: "0 20px 12px", flexShrink: 0 }}>
        <div style={{
          display: "flex", alignItems: "center", gap: 8,
          borderRadius: 10, border: "1px solid rgba(255,255,255,0.08)",
          background: "rgba(255,255,255,0.03)", padding: "8px 12px",
        }}>
          <svg width={14} height={14} viewBox="0 0 24 24" fill="none" strokeWidth={2}
            strokeLinecap="round" strokeLinejoin="round" style={{ stroke: "rgba(255,255,255,0.3)", flexShrink: 0 }}>
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            ref={inputRef}
            data-testid="search-input"
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Escape") setQuery(""); }}
            placeholder="Search dictations, notes, meetings…"
            autoFocus
            spellCheck={false}
            style={{
              flex: 1, background: "none", border: "none", outline: "none",
              color: "#fafaf9", fontSize: 12, fontFamily: "inherit",
            }}
          />
          {query && (
            <button
              onClick={() => { setQuery(""); inputRef.current?.focus(); }}
              title="Clear"
              style={{
                background: "none", border: "none", cursor: "pointer", padding: 0,
                color: "rgba(255,255,255,0.3)", fontSize: 13, lineHeight: 1, fontFamily: "inherit",
              }}
            >
              ×
            </button>
          )}
        </div>
      </div>

      {/* Results */}
      <div style={{ flex: 1, overflowY: "auto", padding: "0 20px 16px" }}>
        {error ? (
          <div style={{ color: "rgba(239,68,68,0.7)", fontSize: 12, padding: 20, textAlign: "center" }}>
            Search failed: {error}
          </div>
        ) : !query.trim() ? (
          <div style={{ color: "rgba(255,255,255,0.2)", fontSize: 12, padding: 40, textAlign: "center" }}>
            Type to search across your captured content
          </div>
        ) : loading ? (
          <div style={{ color: "rgba(255,255,255,0.2)", fontSize: 12, padding: 40, textAlign: "center" }}>
            Searching…
          </div>
        ) : searched && results.length === 0 ? (
          <div style={{ color: "rgba(255,255,255,0.2)", fontSize: 12, padding: 40, textAlign: "center" }}>
            No matches for “{query.trim()}”
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
            {groups.map((group) => (
              <div key={group.type}>
                {/* Group header */}
                <div style={{
                  fontSize: 9, fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.08em",
                  color: STRIPE_COLOR[group.type], marginBottom: 6, display: "flex", alignItems: "center", gap: 6,
                }}>
                  {GROUP_LABEL[group.type]}
                  <span style={{ color: "rgba(255,255,255,0.2)", fontWeight: 400 }}>{group.items.length}</span>
                </div>

                <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  {group.items.map((entry) => {
                    const stripe = STRIPE_COLOR[entry.source_type] || "#a8a29e";
                    const expanded = expandedId === entry.id;
                    const text = entryText(entry);
                    const snip = makeSnippet(text, query.trim());
                    const containerTab = CONTAINER_TAB[entry.source_type];
                    return (
                      <div
                        key={entry.id}
                        data-testid="search-result"
                        onClick={() => setExpandedId(expanded ? null : entry.id)}
                        style={{
                          display: "flex",
                          borderRadius: 10,
                          border: expanded ? "1px solid rgba(245,158,11,0.25)" : "1px solid rgba(255,255,255,0.06)",
                          background: expanded ? "rgba(255,255,255,0.04)" : "rgba(255,255,255,0.025)",
                          overflow: "hidden",
                          minHeight: 44,
                          cursor: "pointer",
                        }}
                      >
                        <div style={{ width: 3, flexShrink: 0, background: stripe }} />
                        <div style={{ flex: 1, padding: "8px 12px", minWidth: 0 }}>
                          {/* Top row */}
                          <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 4 }}>
                            <span style={{ fontSize: 9, color: "rgba(255,255,255,0.25)", fontFamily: "'SF Mono',ui-monospace,monospace" }}>
                              {formatTime(entry.created_at)}
                            </span>
                            <span style={{
                              fontSize: 8, fontWeight: 600, padding: "1px 5px", borderRadius: 3,
                              background: `${stripe}18`, color: stripe, textTransform: "uppercase", letterSpacing: "0.05em",
                            }}>
                              {entry.source_type}
                            </span>
                            {entry.mode && entry.mode !== "raw" && entry.mode !== entry.source_type && (
                              <span style={{ fontSize: 8, color: "rgba(255,255,255,0.15)", background: "rgba(255,255,255,0.03)", padding: "1px 5px", borderRadius: 3 }}>
                                {entry.mode}
                              </span>
                            )}
                          </div>

                          {/* Snippet or full text */}
                          {expanded ? (
                            <div>
                              <div style={{
                                fontSize: 11, lineHeight: "1.6", color: "rgba(255,255,255,0.7)",
                                wordBreak: "break-word", whiteSpace: "pre-wrap",
                              }}>
                                {text || "(no content)"}
                              </div>
                              {containerTab && entry.container_id != null && (
                                <button
                                  onClick={(e) => { e.stopPropagation(); onNavigate(containerTab); }}
                                  style={{
                                    marginTop: 8, padding: "3px 10px", borderRadius: 6, fontSize: 10,
                                    fontWeight: 500, border: "1px solid rgba(245,158,11,0.3)",
                                    background: "rgba(245,158,11,0.1)", color: "#fbbf24",
                                    cursor: "pointer", fontFamily: "inherit",
                                  }}
                                >
                                  Open in {containerTab === "notes" ? "Notes" : "Meetings"} →
                                </button>
                              )}
                            </div>
                          ) : (
                            <div style={{
                              fontSize: 11, lineHeight: "1.5",
                              color: text ? "rgba(255,255,255,0.55)" : "rgba(255,255,255,0.2)",
                              fontStyle: text ? "normal" : "italic",
                              wordBreak: "break-word",
                            }}>
                              {text ? (
                                <>
                                  {snip.before}
                                  {snip.match && (
                                    <span style={{ background: "rgba(245,158,11,0.22)", color: "#fbbf24", borderRadius: 2, padding: "0 1px" }}>
                                      {snip.match}
                                    </span>
                                  )}
                                  {snip.after}
                                </>
                              ) : "(no content)"}
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
    </div>
  );
}
