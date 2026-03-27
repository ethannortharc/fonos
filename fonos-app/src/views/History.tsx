// History view — persistent event history with pagination, filtering, and session grouping.
// Full implementation in WP-08.

import { useState, useEffect, useCallback } from "react";
import { getHistory, deleteEvent, playAudioFile } from "../lib/api";
import type { Event } from "../types";

const PAGE_SIZE = 20;

type TypeFilter = "all" | "stt" | "tts" | "llm";

function groupBySession(events: Event[]): Map<string, Event[]> {
  const groups = new Map<string, Event[]>();
  for (const ev of events) {
    const key = ev.session_id || ev.id.toString();
    const group = groups.get(key) ?? [];
    group.push(ev);
    groups.set(key, group);
  }
  return groups;
}

function relativeTime(isoDate: string): string {
  const d = new Date(isoDate);
  const now = new Date();
  const today = now.toDateString();
  const yesterday = new Date(now.getTime() - 86400000).toDateString();
  const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (d.toDateString() === today) return `Today, ${time}`;
  if (d.toDateString() === yesterday) return `Yesterday, ${time}`;
  return `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })}, ${time}`;
}

/** Colored dot for event type */
function TypeDot({ type }: { type: string }) {
  const color =
    type === "stt"
      ? "bg-[#fbbf24]"
      : type === "tts"
      ? "bg-[#c4b5fd]"
      : "bg-[#86efac]";
  return <span className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${color}`} />;
}

export default function History() {
  const [events, setEvents] = useState<Event[]>([]);
  const [typeFilter, setTypeFilter] = useState<TypeFilter>("all");
  const [page, setPage] = useState<number>(0);
  const [loading, setLoading] = useState<boolean>(false);
  const [hasMore, setHasMore] = useState<boolean>(true);
  const [expandedId, setExpandedId] = useState<number | null>(null);

  const load = useCallback(
    async (p: number, filter: TypeFilter) => {
      setLoading(true);
      try {
        const results = await getHistory(
          PAGE_SIZE,
          p * PAGE_SIZE,
          filter === "all" ? "" : filter
        );
        if (p === 0) {
          setEvents(results);
        } else {
          setEvents((prev) => [...prev, ...results]);
        }
        setHasMore(results.length === PAGE_SIZE);
      } catch (e: unknown) {
        console.error("getHistory:", e);
      } finally {
        setLoading(false);
      }
    },
    []
  );

  useEffect(() => {
    setPage(0);
    load(0, typeFilter);
  }, [typeFilter, load]);

  const handleLoadMore = useCallback(() => {
    const next = page + 1;
    setPage(next);
    load(next, typeFilter);
  }, [page, load, typeFilter]);

  const handleDelete = useCallback(
    async (id: number) => {
      try {
        await deleteEvent(id);
        setEvents((prev) => prev.filter((e) => e.id !== id));
      } catch (e: unknown) {
        console.error("deleteEvent:", e);
      }
    },
    []
  );

  const handlePlay = useCallback(async (audioPath: string) => {
    if (!audioPath) return;
    try {
      await playAudioFile(audioPath);
    } catch (e: unknown) {
      console.error("playAudioFile:", e);
    }
  }, []);

  const sessionGroups = groupBySession(events);
  const sessionKeys = Array.from(
    new Map(events.map((e) => [e.session_id || e.id.toString(), true])).keys()
  );

  return (
    <div className="flex flex-col h-full bg-[#1a1917]">
      {/* Header + filter */}
      <div className="flex items-center justify-between px-5 py-4">
        <h2 className="text-[16px] font-semibold text-[#fafaf9]">History</h2>
        {/* Type filter */}
        <div className="flex gap-1">
          {(["all", "stt", "tts", "llm"] as TypeFilter[]).map((f) => (
            <button
              key={f}
              onClick={() => setTypeFilter(f)}
              className={[
                "px-2.5 py-1 rounded-lg text-[10px] font-medium transition-colors uppercase",
                typeFilter === f
                  ? "bg-[rgba(245,158,11,0.12)] text-[#fbbf24]"
                  : "bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.35)] hover:bg-[rgba(255,255,255,0.07)]",
              ].join(" ")}
            >
              {f}
            </button>
          ))}
        </div>
      </div>

      {/* Event cards — with session grouping */}
      <div className="flex-1 overflow-auto px-5 pb-4 flex flex-col gap-2">
        {sessionKeys.map((sessionKey) => {
          const sessionEvents = sessionGroups.get(sessionKey) ?? [];
          const isMulti = sessionEvents.length > 1;

          return (
            <div
              key={sessionKey}
              className={
                isMulti
                  ? "rounded-lg border border-[rgba(255,255,255,0.06)] bg-[rgba(255,255,255,0.01)]"
                  : ""
              }
            >
              {/* Session header (only for multi-event sessions) */}
              {isMulti && (
                <div className="px-3 py-1.5 border-b border-[rgba(255,255,255,0.05)]">
                  <span className="text-[rgba(255,255,255,0.25)] text-[11px]">
                    Session · {sessionEvents.length} events
                  </span>
                </div>
              )}

              {/* Individual event cards */}
              <div className={isMulti ? "flex flex-col gap-px p-1" : "flex flex-col gap-2"}>
                {sessionEvents.map((ev) => (
                  <div
                    key={ev.id}
                    className="rounded-lg bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] px-3 py-2.5 cursor-pointer hover:bg-[rgba(255,255,255,0.04)] transition-colors"
                    onClick={() =>
                      setExpandedId(expandedId === ev.id ? null : ev.id)
                    }
                  >
                    {/* Top row: dot + timestamp + spacer + metadata + actions */}
                    <div className="flex items-center gap-2">
                      <TypeDot type={ev.type} />
                      <span className="text-[11px] text-[rgba(255,255,255,0.35)]">
                        {relativeTime(ev.created_at)}
                      </span>
                      <span className="flex-1" />
                      <span className="text-[10px] text-[rgba(255,255,255,0.2)]">
                        {ev.duration_secs > 0 && `${ev.duration_secs.toFixed(1)}s`}
                        {ev.duration_secs > 0 && ev.latency_ms > 0 && " · "}
                        {ev.latency_ms > 0 && `${ev.latency_ms}ms`}
                      </span>
                      {ev.audio_path && (
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            handlePlay(ev.audio_path);
                          }}
                          className="text-[rgba(255,255,255,0.2)] hover:text-[rgba(255,255,255,0.5)] text-xs transition-colors"
                        >
                          ▶
                        </button>
                      )}
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          handleDelete(ev.id);
                        }}
                        className="text-[rgba(255,255,255,0.2)] hover:text-[#ef4444] text-xs transition-colors"
                      >
                        ✕
                      </button>
                    </div>

                    {/* Input text preview */}
                    <p className="text-[12px] text-[rgba(255,255,255,0.6)] leading-relaxed line-clamp-2 mt-1">
                      {ev.input_text || ev.output_text}
                    </p>

                    {/* Expanded detail */}
                    {expandedId === ev.id && (
                      <div className="mt-2 flex flex-col gap-1 border-t border-[rgba(255,255,255,0.05)] pt-2">
                        {ev.output_text && ev.output_text !== ev.input_text && (
                          <p className="text-[12px] text-[rgba(255,255,255,0.45)] leading-relaxed">
                            {ev.output_text}
                          </p>
                        )}
                        <div className="flex gap-3 text-[rgba(255,255,255,0.2)] text-[10px]">
                          {ev.mode && <span>Mode: {ev.mode}</span>}
                          {ev.model && <span>Model: {ev.model}</span>}
                          {(ev.tokens_in > 0 || ev.tokens_out > 0) && (
                            <span>
                              {ev.tokens_in}+{ev.tokens_out} tok
                            </span>
                          )}
                        </div>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            </div>
          );
        })}

        {/* Pagination — Load more */}
        {hasMore && !loading && (
          <button
            onClick={handleLoadMore}
            className="w-full py-2 text-[11px] text-[rgba(255,255,255,0.15)] hover:text-[rgba(255,255,255,0.3)] transition-colors"
          >
            Load more
          </button>
        )}
        {loading && (
          <div className="text-center text-[rgba(255,255,255,0.2)] text-[11px] py-2">
            Loading...
          </div>
        )}
        {!hasMore && events.length > 0 && (
          <div className="text-center text-[rgba(255,255,255,0.15)] text-[11px] py-2">
            End of history
          </div>
        )}
        {!loading && events.length === 0 && (
          <div className="text-center text-[rgba(255,255,255,0.2)] text-[11px] py-8">
            No events yet
          </div>
        )}
      </div>
    </div>
  );
}
