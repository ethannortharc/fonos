// Meetings view — two-level: meeting list → meeting detail.
// Uses inline styles (same pattern as Recent.tsx) — NO Tailwind classes for content.

import { useState, useEffect, useCallback, useRef } from "react";
import type { ReactNode } from "react";
import { CheckboxIcon, CheckboxCheckedIcon, BulletIcon } from "../components/Icons";
import {
  getMeetings,
  getMeetingDetail,
  exportMeetingMd,
  exportMeetingJson,
} from "../lib/meeting-api";
import type { MeetingDetail } from "../lib/meeting-api";
import type { Container, Entry } from "../lib/storage-api";
import { deleteContainer } from "../lib/storage-api";

// ─── Helpers ──────────────────────────────────────────────────────────────────

function formatDateTime(iso: string): string {
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

function formatDuration(startIso: string, endIso: string): string {
  try {
    const start = new Date(startIso).getTime();
    const end = new Date(endIso).getTime();
    const diffMs = end - start;
    if (diffMs <= 0) return "";
    const mins = Math.round(diffMs / 60000);
    if (mins < 1) return "< 1 min";
    if (mins === 1) return "1 min";
    return `${mins} min`;
  } catch {
    return "";
  }
}

function summaryPreview(text: string, maxLen = 100): string {
  const t = text?.trim() ?? "";
  if (t.length === 0) return "(no summary)";
  return t.length > maxLen ? t.slice(0, maxLen) + "…" : t;
}

// ─── Markdown renderer ────────────────────────────────────────────────────────
// Minimal inline renderer — no external dependencies.
// Handles: ## headings, **bold**, - bullets, - [ ] / - [x] checkboxes,
// ```code blocks```, and line breaks.

function renderMarkdown(text: string): ReactNode {
  const lines = text.split("\n");
  const nodes: ReactNode[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // Fenced code block
    if (line.trim().startsWith("```")) {
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trim().startsWith("```")) {
        codeLines.push(lines[i]);
        i++;
      }
      nodes.push(
        <pre
          key={`code-${i}`}
          style={{
            fontFamily: "'SF Mono', ui-monospace, monospace",
            fontSize: 11,
            background: "rgba(255,255,255,0.04)",
            border: "1px solid rgba(255,255,255,0.07)",
            borderRadius: 6,
            padding: "8px 10px",
            margin: "4px 0",
            overflowX: "auto",
            color: "rgba(255,255,255,0.7)",
            whiteSpace: "pre",
          }}
        >
          {codeLines.join("\n")}
        </pre>
      );
      i++; // skip closing ```
      continue;
    }

    // ## Heading (h2)
    if (/^##\s/.test(line)) {
      nodes.push(
        <div
          key={`h2-${i}`}
          style={{
            fontSize: 13,
            fontWeight: 700,
            color: "rgba(255,255,255,0.85)",
            marginTop: 10,
            marginBottom: 3,
          }}
        >
          {renderInline(line.replace(/^##\s+/, ""))}
        </div>
      );
      i++;
      continue;
    }

    // # Heading (h1)
    if (/^#\s/.test(line)) {
      nodes.push(
        <div
          key={`h1-${i}`}
          style={{
            fontSize: 14,
            fontWeight: 700,
            color: "rgba(255,255,255,0.9)",
            marginTop: 12,
            marginBottom: 4,
          }}
        >
          {renderInline(line.replace(/^#\s+/, ""))}
        </div>
      );
      i++;
      continue;
    }

    // ### Heading (h3)
    if (/^###\s/.test(line)) {
      nodes.push(
        <div
          key={`h3-${i}`}
          style={{
            fontSize: 12,
            fontWeight: 700,
            color: "rgba(255,255,255,0.8)",
            marginTop: 8,
            marginBottom: 2,
          }}
        >
          {renderInline(line.replace(/^###\s+/, ""))}
        </div>
      );
      i++;
      continue;
    }

    // - [ ] checkbox (unchecked)
    if (/^-\s\[ \]\s/.test(line)) {
      nodes.push(
        <div
          key={`cb-${i}`}
          style={{ display: "flex", alignItems: "flex-start", gap: 6, margin: "2px 0", paddingLeft: 8 }}
        >
          <span style={{ color: "#fbbf24", flexShrink: 0, marginTop: 1 }}><CheckboxIcon size={12} /></span>
          <span style={{ fontSize: 12, color: "rgba(255,255,255,0.65)", lineHeight: "1.5" }}>
            {renderInline(line.replace(/^-\s\[ \]\s/, ""))}
          </span>
        </div>
      );
      i++;
      continue;
    }

    // - [x] checkbox (checked)
    if (/^-\s\[x\]\s/i.test(line)) {
      nodes.push(
        <div
          key={`cbx-${i}`}
          style={{ display: "flex", alignItems: "flex-start", gap: 6, margin: "2px 0", paddingLeft: 8 }}
        >
          <span style={{ color: "#4ade80", flexShrink: 0, marginTop: 1 }}><CheckboxCheckedIcon size={12} /></span>
          <span style={{
            fontSize: 12,
            color: "rgba(255,255,255,0.4)",
            lineHeight: "1.5",
            textDecoration: "line-through",
          }}>
            {renderInline(line.replace(/^-\s\[x\]\s/i, ""))}
          </span>
        </div>
      );
      i++;
      continue;
    }

    // - bullet point
    if (/^-\s/.test(line)) {
      nodes.push(
        <div
          key={`li-${i}`}
          style={{ display: "flex", alignItems: "flex-start", gap: 6, margin: "2px 0", paddingLeft: 8 }}
        >
          <span style={{ color: "rgba(251,191,36,0.6)", flexShrink: 0, marginTop: 2 }}><BulletIcon size={8} /></span>
          <span style={{ fontSize: 12, color: "rgba(255,255,255,0.65)", lineHeight: "1.5" }}>
            {renderInline(line.replace(/^-\s/, ""))}
          </span>
        </div>
      );
      i++;
      continue;
    }

    // Empty line → spacer
    if (line.trim() === "") {
      nodes.push(<div key={`sp-${i}`} style={{ height: 6 }} />);
      i++;
      continue;
    }

    // Regular paragraph text
    nodes.push(
      <p
        key={`p-${i}`}
        style={{
          fontSize: 12,
          lineHeight: "1.6",
          color: "rgba(255,255,255,0.65)",
          margin: "2px 0",
        }}
      >
        {renderInline(line)}
      </p>
    );
    i++;
  }

  return <>{nodes}</>;
}

/** Render inline markdown: **bold**, `code` */
function renderInline(text: string): ReactNode {
  // Split on **bold** and `code` patterns
  const parts = text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g);
  if (parts.length === 1) return text;

  return (
    <>
      {parts.map((part, idx) => {
        if (/^\*\*[^*]+\*\*$/.test(part)) {
          return (
            <strong
              key={idx}
              style={{ fontWeight: 700, color: "rgba(255,255,255,0.85)" }}
            >
              {part.slice(2, -2)}
            </strong>
          );
        }
        if (/^`[^`]+`$/.test(part)) {
          return (
            <code
              key={idx}
              style={{
                fontFamily: "'SF Mono', ui-monospace, monospace",
                fontSize: 11,
                background: "rgba(255,255,255,0.07)",
                borderRadius: 3,
                padding: "1px 4px",
                color: "rgba(255,255,255,0.8)",
              }}
            >
              {part.slice(1, -1)}
            </code>
          );
        }
        return part;
      })}
    </>
  );
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

const EXPORT_ICON = (
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
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
    <polyline points="7 10 12 15 17 10" />
    <line x1="12" y1="15" x2="12" y2="3" />
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

const CHEVRON_DOWN_ICON = (
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

const CHEVRON_RIGHT_ICON = (
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
    <polyline points="9 18 15 12 9 6" />
  </svg>
);

// ─── Meeting Detail ────────────────────────────────────────────────────────────

interface MeetingDetailViewProps {
  meeting: Container;
  onBack: () => void;
  onDeleted: () => void;
}

export function MeetingDetailView({ meeting, onBack, onDeleted }: MeetingDetailViewProps) {
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [summaryOpen, setSummaryOpen] = useState(true);
  const [summaryRendered, setSummaryRendered] = useState(true);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [showExportMenu, setShowExportMenu] = useState(false);
  const exportRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setLoading(true);
    setError("");
    getMeetingDetail(meeting.id)
      .then((d) => setDetail(d))
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [meeting.id]);

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

  const handleExportMd = async () => {
    setShowExportMenu(false);
    setExporting(true);
    try {
      await exportMeetingMd(meeting.id, "");
    } catch (e) {
      console.error("exportMeetingMd:", e);
    } finally {
      setExporting(false);
    }
  };

  const handleExportJson = async () => {
    setShowExportMenu(false);
    setExporting(true);
    try {
      await exportMeetingJson(meeting.id, "");
    } catch (e) {
      console.error("exportMeetingJson:", e);
    } finally {
      setExporting(false);
    }
  };

  const handleDelete = async () => {
    if (!confirmDelete) {
      setConfirmDelete(true);
      setTimeout(() => setConfirmDelete(false), 3000);
      return;
    }
    setDeleting(true);
    try {
      await deleteContainer(meeting.id);
      onDeleted();
    } catch (e) {
      console.error("deleteContainer:", e);
      setDeleting(false);
    }
  };

  // Extract entries grouped by role
  const transcriptEntries: Entry[] = detail
    ? detail.entries.filter((e) => e.role !== "system")
    : [];

  const summaryText = detail?.summary
    ? (detail.summary.processed_text || detail.summary.raw_text)
    : null;

  const duration = formatDuration(meeting.created_at, meeting.updated_at);

  return (
    <div
      data-testid="meeting-detail"
      style={{ display: "flex", flexDirection: "column", height: "100%", background: "#1a1917" }}
    >
      {/* Top bar */}
      <div style={{
        display: "flex", alignItems: "center", gap: 8,
        padding: "10px 16px",
        borderBottom: "1px solid rgba(255,255,255,0.05)",
        flexShrink: 0,
      }}>
        <button
          data-testid="meeting-back-btn"
          onClick={onBack}
          title="Back to meetings"
          style={{
            width: 28, height: 28, borderRadius: 8,
            display: "flex", alignItems: "center", justifyContent: "center",
            background: "none", border: "none", cursor: "pointer",
            color: "rgba(255,255,255,0.4)",
          }}
        >
          {BACK_ICON}
        </button>

        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{
            fontSize: 14, fontWeight: 600,
            color: "#fafaf9",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
          }}>
            {meeting.title || "Untitled Meeting"}
          </div>
          <div style={{ fontSize: 10, color: "rgba(255,255,255,0.3)", marginTop: 1 }}>
            {formatDateTime(meeting.created_at)}{duration ? ` · ${duration}` : ""}
          </div>
        </div>

        {/* Export dropdown */}
        <div style={{ position: "relative" }} ref={exportRef}>
          <button
            data-testid="meeting-export-btn"
            onClick={() => setShowExportMenu((v) => !v)}
            disabled={exporting}
            title="Export meeting"
            style={{
              display: "flex", alignItems: "center", gap: 4,
              padding: "5px 10px", borderRadius: 8,
              background: "none", border: "1px solid rgba(255,255,255,0.08)",
              cursor: "pointer", fontSize: 11,
              color: "rgba(255,255,255,0.4)",
            }}
          >
            {EXPORT_ICON}
            <span style={{ marginLeft: 2 }}>Export</span>
            {CHEVRON_DOWN_ICON}
          </button>

          {showExportMenu && (
            <div style={{
              position: "absolute", right: 0, top: "100%", marginTop: 4,
              width: 160, background: "#242220",
              border: "1px solid rgba(255,255,255,0.1)",
              borderRadius: 8, boxShadow: "0 4px 20px rgba(0,0,0,0.4)",
              zIndex: 50, overflow: "hidden",
            }}>
              <button
                data-testid="meeting-export-md"
                onClick={handleExportMd}
                style={{
                  width: "100%", padding: "10px 12px", textAlign: "left",
                  background: "none", border: "none", cursor: "pointer",
                  fontSize: 12, color: "rgba(255,255,255,0.7)",
                }}
              >
                Export as Markdown
              </button>
              <button
                data-testid="meeting-export-json"
                onClick={handleExportJson}
                style={{
                  width: "100%", padding: "10px 12px", textAlign: "left",
                  background: "none", border: "none", cursor: "pointer",
                  fontSize: 12, color: "rgba(255,255,255,0.7)",
                }}
              >
                Export as JSON
              </button>
            </div>
          )}
        </div>

        {/* Delete button */}
        <button
          data-testid="meeting-delete-btn"
          onClick={handleDelete}
          disabled={deleting}
          title={confirmDelete ? "Click again to confirm delete" : "Delete meeting"}
          style={{
            width: 28, height: 28, borderRadius: 8,
            display: "flex", alignItems: "center", justifyContent: "center",
            background: confirmDelete ? "rgba(239,68,68,0.1)" : "none",
            border: "none", cursor: "pointer",
            color: confirmDelete ? "#ef4444" : "rgba(255,255,255,0.25)",
          }}
        >
          {TRASH_ICON}
        </button>
      </div>

      {/* Content */}
      <div style={{ flex: 1, overflowY: "auto", padding: "16px" }}>
        {loading ? (
          <div style={{ color: "rgba(255,255,255,0.2)", fontSize: 12, textAlign: "center", padding: 40 }}>
            Loading…
          </div>
        ) : error ? (
          <div style={{ color: "rgba(239,68,68,0.7)", fontSize: 12, textAlign: "center", padding: 20 }}>
            Error: {error}
          </div>
        ) : detail ? (
          <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
            {/* Meta strip — the at-a-glance facts: when, how long, how much */}
            {transcriptEntries.length > 0 && (
              <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                {[
                  formatDateTime(transcriptEntries[0].created_at),
                  formatDuration(
                    transcriptEntries[0].created_at,
                    transcriptEntries[transcriptEntries.length - 1].created_at
                  ),
                  `${transcriptEntries.length} segment${transcriptEntries.length === 1 ? "" : "s"}`,
                ].map((label, i) => (
                  <span
                    key={i}
                    style={{
                      fontSize: 10, padding: "3px 9px", borderRadius: 12,
                      background: "rgba(255,255,255,0.04)",
                      border: "1px solid rgba(255,255,255,0.05)",
                      color: "rgba(255,255,255,0.45)",
                      fontFamily: i === 0 ? "inherit" : "'SF Mono', ui-monospace, monospace",
                    }}
                  >
                    {label}
                  </span>
                ))}
              </div>
            )}

            {/* AI Summary section (collapsible) */}
            <div style={{
              borderRadius: 10,
              border: "1px solid rgba(251,191,36,0.15)",
              background: "rgba(251,191,36,0.04)",
              overflow: "hidden",
            }}>
              <button
                data-testid="meeting-summary-toggle"
                onClick={() => setSummaryOpen((v) => !v)}
                style={{
                  width: "100%", display: "flex", alignItems: "center", gap: 8,
                  padding: "10px 12px", background: "none", border: "none",
                  cursor: "pointer", textAlign: "left",
                }}
              >
                <span style={{ color: "rgba(251,191,36,0.5)" }}>
                  {summaryOpen ? CHEVRON_DOWN_ICON : CHEVRON_RIGHT_ICON}
                </span>
                <span style={{ fontSize: 11, fontWeight: 600, color: "rgba(251,191,36,0.7)", letterSpacing: "0.05em", textTransform: "uppercase" }}>
                  AI Summary
                </span>
                {!summaryText && (
                  <span style={{ fontSize: 10, color: "rgba(255,255,255,0.2)", fontStyle: "italic" }}>
                    not generated
                  </span>
                )}
              </button>

              {summaryOpen && (
                <div style={{ padding: "0 12px 12px" }}>
                  {summaryText ? (
                    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                      {/* Raw / Rendered toggle */}
                      <div style={{ display: "flex", justifyContent: "flex-end" }}>
                        <div style={{
                          display: "flex",
                          borderRadius: 6,
                          border: "1px solid rgba(255,255,255,0.08)",
                          overflow: "hidden",
                        }}>
                          <button
                            data-testid="summary-rendered-btn"
                            onClick={() => setSummaryRendered(true)}
                            style={{
                              padding: "3px 8px",
                              fontSize: 10,
                              background: summaryRendered ? "rgba(251,191,36,0.15)" : "none",
                              border: "none",
                              cursor: "pointer",
                              color: summaryRendered ? "#fbbf24" : "rgba(255,255,255,0.3)",
                              fontWeight: summaryRendered ? 600 : 400,
                            }}
                          >
                            Rendered
                          </button>
                          <button
                            data-testid="summary-raw-btn"
                            onClick={() => setSummaryRendered(false)}
                            style={{
                              padding: "3px 8px",
                              fontSize: 10,
                              background: !summaryRendered ? "rgba(251,191,36,0.15)" : "none",
                              border: "none",
                              cursor: "pointer",
                              color: !summaryRendered ? "#fbbf24" : "rgba(255,255,255,0.3)",
                              fontWeight: !summaryRendered ? 600 : 400,
                            }}
                          >
                            Raw
                          </button>
                        </div>
                      </div>

                      {/* Summary content */}
                      {summaryRendered ? (
                        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                          {renderMarkdown(summaryText)}
                        </div>
                      ) : (
                        <pre style={{
                          fontSize: 12,
                          lineHeight: "1.6",
                          color: "rgba(255,255,255,0.65)",
                          whiteSpace: "pre-wrap",
                          wordBreak: "break-word",
                          margin: 0,
                          fontFamily: "'SF Mono', ui-monospace, monospace",
                          background: "rgba(255,255,255,0.03)",
                          borderRadius: 6,
                          padding: "8px 10px",
                        }}>
                          {summaryText}
                        </pre>
                      )}
                    </div>
                  ) : (
                    <div style={{ fontSize: 12, color: "rgba(255,255,255,0.2)", fontStyle: "italic" }}>
                      Summary not generated
                    </div>
                  )}
                </div>
              )}
            </div>

            {/* Transcript Timeline */}
            <div>
              <div style={{
                fontSize: 10, fontWeight: 600, color: "rgba(255,255,255,0.3)",
                letterSpacing: "0.05em", textTransform: "uppercase", marginBottom: 8,
              }}>
                Transcript
              </div>

              {transcriptEntries.length === 0 ? (
                <div style={{ fontSize: 12, color: "rgba(255,255,255,0.2)", fontStyle: "italic", padding: "8px 0" }}>
                  No transcript entries
                </div>
              ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  {transcriptEntries.map((entry) => {
                    const isMe = entry.role === "user";
                    const speakerColor = isMe ? "#4ade80" : "#c084fc";
                    const speakerLabel = isMe ? "Me" : "Other";
                    const text = entry.processed_text || entry.raw_text;

                    return (
                      <div
                        key={entry.id}
                        style={{
                          display: "flex",
                          gap: 10,
                          padding: "8px 10px",
                          borderRadius: 8,
                          background: "rgba(255,255,255,0.02)",
                          border: "1px solid rgba(255,255,255,0.04)",
                        }}
                      >
                        {/* Timestamp */}
                        <span style={{
                          fontSize: 9, color: "rgba(255,255,255,0.25)",
                          fontFamily: "'SF Mono', ui-monospace, monospace",
                          flexShrink: 0, paddingTop: 2, minWidth: 70,
                        }}>
                          {new Date(entry.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                        </span>

                        {/* Speaker badge */}
                        <span style={{
                          fontSize: 9, fontWeight: 600, padding: "1px 6px",
                          borderRadius: 3, flexShrink: 0,
                          background: `${speakerColor}18`, color: speakerColor,
                          textTransform: "uppercase", letterSpacing: "0.05em",
                          alignSelf: "flex-start", marginTop: 1,
                        }}>
                          {speakerLabel}
                        </span>

                        {/* Text */}
                        <p style={{
                          flex: 1, margin: 0,
                          fontSize: 12, lineHeight: "1.5",
                          color: "rgba(255,255,255,0.6)",
                          wordBreak: "break-word",
                        }}>
                          {text || <span style={{ color: "rgba(255,255,255,0.2)", fontStyle: "italic" }}>(no text)</span>}
                        </p>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}

// ─── Meeting List ──────────────────────────────────────────────────────────────

interface MeetingListProps {
  onSelectMeeting: (meeting: Container) => void;
  refreshKey: number;
}

function MeetingList({ onSelectMeeting, refreshKey, embedded }: MeetingListProps & { embedded?: boolean }) {
  const [meetings, setMeetings] = useState<Container[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const results = await getMeetings();
      // Sort newest first
      const sorted = [...results].sort(
        (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
      );
      setMeetings(sorted);
    } catch (e) {
      console.error("getMeetings:", e);
      setError(String(e));
      setMeetings([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load, refreshKey]);

  return (
    <div
      data-testid="meetings-list"
      style={{ display: "flex", flexDirection: "column", height: "100%", background: "#1a1917" }}
    >
      {/* Header */}
      <div style={{ padding: "16px 20px 8px", flexShrink: 0 }}>
        {!embedded && <h2 style={{ fontSize: 13, fontWeight: 600, color: "#fafaf9", margin: 0 }}>Meetings</h2>}
      </div>

      {/* Content */}
      <div style={{ flex: 1, overflowY: "auto", padding: "4px 20px 16px" }}>
        {error ? (
          <div style={{ color: "rgba(239,68,68,0.7)", fontSize: 12, padding: 20, textAlign: "center" }}>
            Error loading meetings: {error}
          </div>
        ) : loading ? (
          <div style={{ color: "rgba(255,255,255,0.2)", fontSize: 12, padding: 40, textAlign: "center" }}>
            Loading…
          </div>
        ) : meetings.length === 0 ? (
          <div style={{ color: "rgba(255,255,255,0.2)", fontSize: 12, padding: 40, textAlign: "center" }}>
            No meetings recorded yet. Press Option+M to start.
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {meetings.map((meeting) => {
              const duration = formatDuration(meeting.created_at, meeting.updated_at);
              // Check summary_generated flag first (set by stop_meeting when a summary is stored).
              // Fall back to summary_preview text if present (legacy or future use).
              const summaryGenerated = !!(meeting.metadata as Record<string, unknown>)?.summary_generated;
              const summaryPreviewText = (meeting.metadata as Record<string, string>)?.summary_preview ?? "";
              const preview = summaryGenerated
                ? (summaryPreviewText ? summaryPreview(summaryPreviewText) : "Summary available")
                : summaryPreview(summaryPreviewText);

              return (
                <button
                  key={meeting.id}
                  data-testid="meeting-card"
                  onClick={() => onSelectMeeting(meeting)}
                  style={{
                    display: "flex",
                    borderRadius: 10,
                    border: "1px solid rgba(255,255,255,0.06)",
                    background: "rgba(255,255,255,0.025)",
                    overflow: "hidden",
                    minHeight: 62,
                    textAlign: "left",
                    cursor: "pointer",
                    width: "100%",
                  }}
                >
                  {/* Orange left stripe */}
                  <div style={{ width: 3, flexShrink: 0, background: "#fbbf24" }} />

                  {/* Content */}
                  <div style={{ flex: 1, padding: "10px 12px", minWidth: 0 }}>
                    {/* Top row: title */}
                    <div style={{
                      fontSize: 12, fontWeight: 600,
                      color: "rgba(255,255,255,0.8)",
                      overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                      marginBottom: 4,
                    }}>
                      {meeting.title || "Untitled Meeting"}
                    </div>

                    {/* Second row: date + duration */}
                    <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 4 }}>
                      <span style={{
                        fontSize: 9, color: "rgba(255,255,255,0.3)",
                        fontFamily: "'SF Mono', ui-monospace, monospace",
                      }}>
                        {formatDateTime(meeting.created_at)}
                      </span>
                      {duration && (
                        <>
                          <span style={{ color: "rgba(255,255,255,0.15)", fontSize: 9 }}>·</span>
                          <span style={{
                            fontSize: 9, color: "#fbbf24",
                            background: "rgba(251,191,36,0.1)",
                            padding: "1px 5px", borderRadius: 3,
                          }}>
                            {duration}
                          </span>
                        </>
                      )}
                    </div>

                    {/* Summary preview */}
                    <div style={{
                      fontSize: 11, lineHeight: "1.4",
                      color: preview === "(no summary)"
                        ? "rgba(255,255,255,0.2)"
                        : "rgba(255,255,255,0.45)",
                      fontStyle: preview === "(no summary)" ? "italic" : "normal",
                      overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                    }}>
                      {preview}
                    </div>
                  </div>
                </button>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

// ─── Meetings root (manages list/detail state) ────────────────────────────────

export default function Meetings({
  embedded,
  onOpenDetail,
}: {
  embedded?: boolean;
  onOpenDetail?: (meeting: Container) => void;
} = {}) {
  const [view, setView] = useState<"list" | "detail">("list");
  const [selectedMeeting, setSelectedMeeting] = useState<Container | null>(null);
  const [listRefreshKey, setListRefreshKey] = useState(0);

  const handleSelectMeeting = (meeting: Container) => {
    // Embedded in History: the parent owns the detail stack.
    if (onOpenDetail) {
      onOpenDetail(meeting);
      return;
    }
    setSelectedMeeting(meeting);
    setView("detail");
  };

  const handleBack = () => {
    setView("list");
    setSelectedMeeting(null);
  };

  const handleDeleted = () => {
    setListRefreshKey((k) => k + 1);
    setView("list");
    setSelectedMeeting(null);
  };

  if (view === "detail" && selectedMeeting) {
    return (
      <MeetingDetailView
        meeting={selectedMeeting}
        onBack={handleBack}
        onDeleted={handleDeleted}
      />
    );
  }

  return (
    <MeetingList
      onSelectMeeting={handleSelectMeeting}
      refreshKey={listRefreshKey}
    embedded={embedded}
    />
  );
}
