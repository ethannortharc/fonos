// Stats view — usage statistics with today card, weekly chart, and period selector.
// Full implementation in WP-09.

import { useState, useEffect, useCallback } from "react";
import { getStats, getToday } from "../lib/api";
import type { DailyStat, TodaySummary } from "../types";

type Period = "7d" | "30d" | "90d";

function formatDate(d: Date): string {
  return d.toISOString().split("T")[0];
}

function getPeriodDates(period: Period): { from: string; to: string } {
  const to = new Date();
  const from = new Date();
  if (period === "7d") from.setDate(from.getDate() - 6);
  else if (period === "30d") from.setDate(from.getDate() - 29);
  else from.setDate(from.getDate() - 89);
  return { from: formatDate(from), to: formatDate(to) };
}

/** Format seconds into a human-friendly string, e.g. 240 -> "4m", 3661 -> "1h 1m" */
function formatTimeSaved(secs: number): string {
  const rounded = Math.round(secs);
  if (rounded < 60) return `${rounded}s`;
  const m = Math.floor(rounded / 60);
  const s = rounded % 60;
  if (m < 60) return s > 0 ? `${m}m ${s}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const rm = m % 60;
  return rm > 0 ? `${h}h ${rm}m` : `${h}h`;
}

// ─── CSS div bar chart ───────────────────────────────────────────────────────

function BarChart({ stats }: { stats: DailyStat[] }) {
  if (stats.length === 0) {
    return (
      <div className="h-[60px] flex items-center justify-center">
        <span className="text-[rgba(255,255,255,0.2)] text-xs">No data</span>
      </div>
    );
  }

  const maxWords = Math.max(...stats.map((s) => s.stt_words), 1);

  return (
    <div>
      <div className="flex items-end gap-1 h-[60px]">
        {stats.map((s) => {
          const ratio = s.stt_words / maxWords;
          const heightPct = Math.max(ratio * 100, 3); // min 3% so bars are visible
          const opacity = 0.2 + ratio * 0.8;
          return (
            <div
              key={s.date}
              className="flex-1 rounded-t-[3px]"
              style={{
                height: `${heightPct}%`,
                backgroundColor: `rgba(251, 191, 36, ${opacity})`,
              }}
            />
          );
        })}
      </div>
      <div className="flex justify-between mt-1.5">
        {stats.length <= 14 ? (
          stats.map((s) => (
            <span
              key={s.date}
              className="flex-1 text-center text-[9px] text-[rgba(255,255,255,0.15)]"
            >
              {s.date.slice(5)}
            </span>
          ))
        ) : (
          <>
            <span className="text-[9px] text-[rgba(255,255,255,0.15)]">
              {stats[0].date.slice(5)}
            </span>
            <span className="text-[9px] text-[rgba(255,255,255,0.15)]">
              {stats[stats.length - 1].date.slice(5)}
            </span>
          </>
        )}
      </div>
    </div>
  );
}

// ─── Stats view ───────────────────────────────────────────────────────────────

export default function Stats() {
  const [period, setPeriod] = useState<Period>("7d");
  const [stats, setStats] = useState<DailyStat[]>([]);
  const [today, setToday] = useState<TodaySummary | null>(null);
  const [loading, setLoading] = useState<boolean>(false);

  const load = useCallback(async (p: Period) => {
    setLoading(true);
    try {
      const { from, to } = getPeriodDates(p);
      const [dailyStats, todaySummary] = await Promise.all([
        getStats(from, to),
        getToday(),
      ]);
      setStats(dailyStats);
      setToday(todaySummary);
    } catch (e: unknown) {
      console.error("getStats:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load(period);
  }, [period, load]);

  const totalSessions = stats.reduce(
    (s, d) => s + d.stt_count + d.tts_count + d.llm_count,
    0
  );
  const timeSaved = stats.reduce((s, d) => s + d.time_saved_secs, 0);

  return (
    <div className="flex flex-col h-full p-5 gap-3 bg-[#1a1917] overflow-auto">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h2 className="text-[16px] font-semibold text-[#fafaf9]">Stats</h2>

        {/* Period selector */}
        <div className="flex gap-1">
          {(["7d", "30d", "90d"] as Period[]).map((p) => (
            <button
              key={p}
              onClick={() => setPeriod(p)}
              className={[
                "px-2.5 py-1 text-[10px] font-medium transition-colors rounded-lg",
                period === p
                  ? "bg-[rgba(245,158,11,0.12)] text-[#fbbf24]"
                  : "bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.35)] hover:bg-[rgba(255,255,255,0.08)]",
              ].join(" ")}
            >
              {p}
            </button>
          ))}
        </div>
      </div>

      {/* Summary tiles */}
      {today && (
        <div className="grid grid-cols-3 gap-2">
          <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)] rounded-[10px] p-3.5 flex flex-col gap-1">
            <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
              Today words
            </span>
            <span className="text-[18px] font-semibold text-[#fafaf9]">
              {today.total_words.toLocaleString()}
            </span>
            <span className="text-[10px] text-[rgba(255,255,255,0.2)]">
              {today.stt_words} STT · {today.tts_words} TTS
            </span>
          </div>
          <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)] rounded-[10px] p-3.5 flex flex-col gap-1">
            <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
              Sessions
            </span>
            <span className="text-[18px] font-semibold text-[#fafaf9]">
              {today.total_sessions}
            </span>
            <span className="text-[10px] text-[rgba(255,255,255,0.2)]">
              {totalSessions} in {period}
            </span>
          </div>
          <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)] rounded-[10px] p-3.5 flex flex-col gap-1">
            <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
              Time saved
            </span>
            <span className="text-[18px] font-semibold text-[#fafaf9]">
              {formatTimeSaved(today.time_saved_secs)}
            </span>
            <span className="text-[10px] text-[rgba(255,255,255,0.2)]">
              {formatTimeSaved(timeSaved)} in {period}
            </span>
          </div>
        </div>
      )}

      {/* Bar chart */}
      <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)] rounded-[10px] p-3.5">
        <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] mb-2 block">
          {period} · STT words per day
        </span>
        {loading ? (
          <div className="h-[60px] flex items-center justify-center">
            <span className="text-[rgba(255,255,255,0.2)] text-xs">Loading...</span>
          </div>
        ) : (
          <BarChart stats={stats} />
        )}
      </div>

      {/* Breakdown row */}
      <div className="flex gap-4 px-1 py-1">
        <div className="flex items-center gap-1.5">
          <span className="w-1.5 h-1.5 rounded-full bg-[#fbbf24]" />
          <span className="text-[10px] text-[rgba(255,255,255,0.35)]">
            {stats.reduce((s, d) => s + d.stt_count, 0)} STT &middot;{" "}
            {stats.reduce((s, d) => s + d.stt_words, 0).toLocaleString()} words
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          <span className="w-1.5 h-1.5 rounded-full bg-[#c4b5fd]" />
          <span className="text-[10px] text-[rgba(255,255,255,0.35)]">
            {stats.reduce((s, d) => s + d.tts_count, 0)} TTS &middot;{" "}
            {stats.reduce((s, d) => s + d.tts_words, 0).toLocaleString()} words
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          <span className="w-1.5 h-1.5 rounded-full bg-[#86efac]" />
          <span className="text-[10px] text-[rgba(255,255,255,0.35)]">
            {stats.reduce((s, d) => s + d.llm_count, 0)} LLM &middot;{" "}
            {stats.reduce((s, d) => s + d.tokens_total, 0).toLocaleString()} tokens
          </span>
        </div>
      </div>
    </div>
  );
}
