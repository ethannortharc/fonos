// Recent view — reverse-chronological timeline of all entries.
// Uses inline styles to avoid Tailwind v4 utility generation issues.

import { useState, useEffect, useCallback } from "react";
import { listEntries } from "../lib/storage-api";
import type { Entry, SourceType } from "../lib/storage-api";

const PAGE_SIZE = 20;

type FilterType = "all" | SourceType;

const FILTERS: { id: FilterType; label: string }[] = [
  { id: "all", label: "All" },
  { id: "dictation", label: "Dictation" },
  { id: "agent", label: "Agent" },
  { id: "note", label: "Note" },
  { id: "meeting", label: "Meeting" },
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
  } catch { return iso; }
}

function preview(e: Entry): string {
  const t = e.processed_text || e.raw_text || "";
  return t.length > 150 ? t.slice(0, 150) + "…" : t;
}

export default function Recent() {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [filter, setFilter] = useState<FilterType>("all");
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async (p: number, f: FilterType) => {
    setLoading(true);
    setError("");
    try {
      const sourceFilter = f === "all" ? undefined : f;
      console.log(`[Recent] loading page=${p} filter=${sourceFilter ?? "all"}`);
      const results = await listEntries(PAGE_SIZE, p * PAGE_SIZE, sourceFilter);
      console.log(`[Recent] got ${results.length} entries`);
      setEntries(results);
    } catch (err) {
      console.error("listEntries error:", err);
      setError(String(err));
      setEntries([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    setPage(0);
    load(0, filter);
  }, [filter, load]);

  const goPage = (p: number) => {
    setPage(p);
    load(p, filter);
  };

  const hasNext = entries.length === PAGE_SIZE;
  const hasPrev = page > 0;

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", background: "#1a1917" }}>
      {/* Header */}
      <div style={{ padding: "16px 20px 8px", flexShrink: 0 }}>
        <h2 style={{ fontSize: 13, fontWeight: 600, color: "#fafaf9", margin: 0 }}>Recent</h2>
      </div>

      {/* Filter pills */}
      <div style={{ display: "flex", gap: 6, padding: "0 20px 12px", flexShrink: 0, flexWrap: "wrap" }}>
        {FILTERS.map((f) => (
          <button
            key={f.id}
            onClick={() => setFilter(f.id)}
            style={{
              padding: "4px 12px",
              borderRadius: 20,
              fontSize: 10,
              fontWeight: 500,
              border: filter === f.id ? "1px solid rgba(245,158,11,0.3)" : "1px solid transparent",
              background: filter === f.id ? "rgba(245,158,11,0.15)" : "rgba(255,255,255,0.04)",
              color: filter === f.id ? "#fbbf24" : "rgba(255,255,255,0.35)",
              cursor: "pointer",
              fontFamily: "inherit",
            }}
          >
            {f.label}
          </button>
        ))}
      </div>

      {/* Entry list */}
      <div style={{ flex: 1, overflowY: "auto", padding: "0 20px 16px" }}>
        {error ? (
          <div style={{ color: "rgba(239,68,68,0.7)", fontSize: 12, padding: 20, textAlign: "center" }}>
            Error loading entries: {error}
          </div>
        ) : loading ? (
          <div style={{ color: "rgba(255,255,255,0.2)", fontSize: 12, padding: 40, textAlign: "center" }}>
            Loading…
          </div>
        ) : entries.length === 0 ? (
          <div style={{ color: "rgba(255,255,255,0.2)", fontSize: 12, padding: 40, textAlign: "center" }}>
            {filter === "all" ? "No entries yet" : `No ${filter} entries`}
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {entries.map((entry) => {
              const text = preview(entry);
              const stripe = STRIPE_COLOR[entry.source_type] || "#a8a29e";
              return (
                <div
                  key={entry.id}
                  style={{
                    display: "flex",
                    borderRadius: 10,
                    border: "1px solid rgba(255,255,255,0.06)",
                    background: "rgba(255,255,255,0.025)",
                    overflow: "hidden",
                    minHeight: 44,
                  }}
                >
                  {/* Color stripe */}
                  <div style={{ width: 3, flexShrink: 0, background: stripe }} />

                  {/* Content */}
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
                    {/* Text */}
                    <div style={{
                      fontSize: 11,
                      lineHeight: "1.5",
                      color: text ? "rgba(255,255,255,0.55)" : "rgba(255,255,255,0.2)",
                      fontStyle: text ? "normal" : "italic",
                      wordBreak: "break-word",
                    }}>
                      {text || "(no content)"}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Pagination */}
      {(hasPrev || hasNext) && (
        <div style={{
          display: "flex", justifyContent: "center", alignItems: "center", gap: 12,
          padding: "10px 20px", borderTop: "1px solid rgba(255,255,255,0.04)", flexShrink: 0,
        }}>
          <button
            onClick={() => goPage(page - 1)}
            disabled={!hasPrev}
            style={{
              fontSize: 11, color: hasPrev ? "rgba(255,255,255,0.4)" : "rgba(255,255,255,0.12)",
              background: "none", border: "none", cursor: hasPrev ? "pointer" : "default", fontFamily: "inherit",
            }}
          >
            ← Previous
          </button>
          <span style={{ fontSize: 11, color: "rgba(255,255,255,0.25)" }}>
            Page {page + 1}
          </span>
          <button
            onClick={() => goPage(page + 1)}
            disabled={!hasNext}
            style={{
              fontSize: 11, color: hasNext ? "rgba(255,255,255,0.4)" : "rgba(255,255,255,0.12)",
              background: "none", border: "none", cursor: hasNext ? "pointer" : "default", fontFamily: "inherit",
            }}
          >
            Next →
          </button>
        </div>
      )}
    </div>
  );
}
