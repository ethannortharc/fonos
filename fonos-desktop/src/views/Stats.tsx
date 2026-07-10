// Stats view — hero metric, KPI tiles, daily activity chart, activity mix.
// Chart colors validated for the dark surface (#1a1917): series #d97706,
// categorical trio #d97706 / #8b5cf6 / #16a34a (lightness band, CVD, contrast).

import { useState, useEffect, useCallback, useMemo } from "react";
import { getStats, getToday, getDictationLatency } from "../lib/api";
import { t, useT } from "../lib/i18n";
import type { DailyStat, TodaySummary, LatencyStats } from "../types";

type Period = "7d" | "30d" | "90d";
type Metric = "words" | "sessions" | "time";

const SERIES = "#d97706";
const MIX = [
  { key: "stt", label: "stats.mix.dictation", color: "#d97706" },
  { key: "tts", label: "stats.mix.speech", color: "#8b5cf6" },
  { key: "llm", label: "stats.mix.llm", color: "#16a34a" },
] as const;

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

/** 240 -> "4m", 3661 -> "1h 1m" */
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

function shortDay(iso: string): string {
  const d = new Date(iso + "T00:00:00");
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}

function metricOf(s: DailyStat, m: Metric): number {
  if (m === "words") return s.stt_words;
  if (m === "sessions") return s.stt_count + s.tts_count + s.llm_count;
  return s.time_saved_secs;
}

function formatMetric(v: number, m: Metric): string {
  return m === "time" ? formatTimeSaved(v) : v.toLocaleString();
}

// ─── Bar chart with hover tooltip ─────────────────────────────────────────────

function BarChart({ stats, metric }: { stats: DailyStat[]; metric: Metric }) {
  const [hover, setHover] = useState<number | null>(null);

  const values = stats.map((s) => metricOf(s, metric));
  const max = Math.max(...values, 1);
  const peakIdx = values.indexOf(Math.max(...values));

  if (stats.length === 0 || values.every((v) => v === 0)) {
    return (
      <div className="h-[130px] flex items-center justify-center">
        <span className="text-[rgba(255,255,255,0.2)] text-[11px]">{t("stats.no-activity")}</span>
      </div>
    );
  }

  const hovered = hover != null ? stats[hover] : null;

  return (
    <div className="relative">
      {/* Tooltip */}
      {hovered && hover != null && (
        <div
          className="absolute z-10 pointer-events-none rounded-lg border border-[rgba(255,255,255,0.1)] bg-[#242220] px-2.5 py-2 shadow-xl"
          style={{
            left: `${Math.min(Math.max(((hover + 0.5) / stats.length) * 100, 12), 82)}%`,
            transform: "translateX(-50%)",
            top: -8,
          }}
        >
          <div className="text-[9px] text-[rgba(255,255,255,0.4)] font-medium mb-1 whitespace-nowrap">
            {shortDay(hovered.date)}
          </div>
          <div className="flex flex-col gap-0.5 text-[10px] whitespace-nowrap">
            <span className="text-[rgba(255,255,255,0.75)]">{hovered.stt_words.toLocaleString()} {t("stats.unit.words")}</span>
            <span className="text-[rgba(255,255,255,0.5)]">
              {hovered.stt_count + hovered.tts_count + hovered.llm_count} {t("stats.unit.sessions")}
            </span>
            <span className="text-[rgba(255,255,255,0.5)]">{formatTimeSaved(hovered.time_saved_secs)} {t("stats.unit.saved")}</span>
          </div>
        </div>
      )}

      {/* Plot: recessive gridline at max + baseline; bars with 2px gaps */}
      <div className="relative h-[130px]">
        <div className="absolute inset-x-0 top-0 border-t border-dashed border-[rgba(255,255,255,0.06)]" />
        <div className="absolute inset-0 flex items-end gap-[2px]">
          {stats.map((s, i) => {
            const v = values[i];
            const h = v === 0 ? 0 : Math.max((v / max) * 100, 2);
            return (
              <div
                key={s.date}
                className="flex-1 h-full flex flex-col justify-end cursor-default"
                onMouseEnter={() => setHover(i)}
                onMouseLeave={() => setHover(null)}
              >
                {/* Selective direct label: peak only */}
                {i === peakIdx && v > 0 && hover == null && (
                  <span className="text-[8px] text-[rgba(255,255,255,0.45)] text-center mb-0.5 whitespace-nowrap overflow-visible">
                    {formatMetric(v, metric)}
                  </span>
                )}
                <div
                  className="rounded-t-[4px] transition-opacity w-full max-w-[40px] mx-auto"
                  style={{
                    height: `${h}%`,
                    background: SERIES,
                    opacity: hover == null ? 0.85 : hover === i ? 1 : 0.35,
                  }}
                />
              </div>
            );
          })}
        </div>
      </div>
      <div className="border-t border-[rgba(255,255,255,0.08)]" />

      {/* X labels: every day for short ranges, endpoints otherwise */}
      <div className="flex justify-between mt-1.5">
        {stats.length <= 10 ? (
          stats.map((s) => (
            <span key={s.date} className="flex-1 text-center text-[9px] text-[rgba(255,255,255,0.22)]">
              {s.date.slice(5)}
            </span>
          ))
        ) : (
          <>
            <span className="text-[9px] text-[rgba(255,255,255,0.22)]">{shortDay(stats[0].date)}</span>
            <span className="text-[9px] text-[rgba(255,255,255,0.22)]">{shortDay(stats[stats.length - 1].date)}</span>
          </>
        )}
      </div>
    </div>
  );
}

// ─── Activity mix: composition bar with gaps + legend ─────────────────────────

function ActivityMix({ stats }: { stats: DailyStat[] }) {
  const counts = {
    stt: stats.reduce((s, d) => s + d.stt_count, 0),
    tts: stats.reduce((s, d) => s + d.tts_count, 0),
    llm: stats.reduce((s, d) => s + d.llm_count, 0),
  };
  const total = counts.stt + counts.tts + counts.llm;
  if (total === 0) return null;

  return (
    <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)] rounded-[10px] p-3.5">
      <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] mb-2.5 block">
        {t("stats.activity-mix")}
      </span>
      {/* Composition bar: 2px surface gaps between segments */}
      <div className="flex h-[10px] gap-[2px] rounded-full overflow-hidden mb-2.5">
        {MIX.map((m) => {
          const v = counts[m.key];
          if (v === 0) return null;
          return (
            <div
              key={m.key}
              title={`${t(m.label as any)}: ${v}`}
              style={{ width: `${(v / total) * 100}%`, background: m.color, minWidth: 3 }}
              className="first:rounded-l-full last:rounded-r-full"
            />
          );
        })}
      </div>
      {/* Legend with values — identity never color-alone */}
      <div className="flex gap-5 flex-wrap">
        {MIX.map((m) => (
          <div key={m.key} className="flex items-center gap-1.5">
            <span className="w-2 h-2 rounded-[3px]" style={{ background: m.color }} />
            <span className="text-[10px] text-[rgba(255,255,255,0.55)]">{t(m.label as any)}</span>
            <span className="text-[10px] text-[rgba(255,255,255,0.3)] font-mono">
              {counts[m.key].toLocaleString()}
            </span>
            <span className="text-[9px] text-[rgba(255,255,255,0.2)]">
              {Math.round((counts[m.key] / total) * 100)}%
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─── Stats view ───────────────────────────────────────────────────────────────

export default function Stats() {
  useT();
  const [period, setPeriod] = useState<Period>("7d");
  const [metric, setMetric] = useState<Metric>("words");
  const [stats, setStats] = useState<DailyStat[]>([]);
  const [today, setToday] = useState<TodaySummary | null>(null);
  const [latency, setLatency] = useState<LatencyStats | null>(null);
  const [loading, setLoading] = useState<boolean>(false);

  const load = useCallback(async (p: Period) => {
    setLoading(true);
    try {
      const { from, to } = getPeriodDates(p);
      const [dailyStats, todaySummary, latencyStats] = await Promise.all([
        getStats(from, to),
        getToday(),
        getDictationLatency(from, to).catch(() => null),
      ]);
      setStats(dailyStats);
      setToday(todaySummary);
      setLatency(latencyStats);
    } catch (e: unknown) {
      console.error("getStats:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load(period);
  }, [period, load]);

  const agg = useMemo(
    () => ({
      words: stats.reduce((s, d) => s + d.stt_words, 0),
      sessions: stats.reduce((s, d) => s + d.stt_count + d.tts_count + d.llm_count, 0),
      timeSaved: stats.reduce((s, d) => s + d.time_saved_secs, 0),
      tokens: stats.reduce((s, d) => s + d.tokens_total, 0),
      activeDays: stats.filter((d) => d.stt_count + d.tts_count + d.llm_count > 0).length,
    }),
    [stats]
  );

  const kpis = today
    ? [
        {
          label: t("stats.kpi.words"),
          value: agg.words.toLocaleString(),
          sub: `${today.total_words.toLocaleString()} ${t("stats.today")}`,
        },
        {
          label: t("stats.kpi.sessions"),
          value: agg.sessions.toLocaleString(),
          sub: `${today.total_sessions} ${t("stats.today")} · ${agg.activeDays} ${agg.activeDays === 1 ? t("stats.active-day") : t("stats.active-days")}`,
        },
        {
          label: t("stats.kpi.llm-latency"),
          value: today.llm_latency_avg > 0 ? `${Math.round(today.llm_latency_avg)}ms` : "—",
          sub: t("stats.avg-today"),
        },
        {
          label: t("stats.kpi.tokens"),
          value: agg.tokens.toLocaleString(),
          sub: `${today.tokens_total.toLocaleString()} ${t("stats.today")}`,
        },
      ]
    : [];

  return (
    <div className="flex flex-col h-full p-5 gap-3 bg-[var(--bg)] overflow-auto">
      {/* Header + period filter */}
      <div className="flex items-center justify-between flex-shrink-0">
        <h2 className="fonos-page-title">{t("stats.title")}</h2>
        <div className="flex gap-1">
          {(["7d", "30d", "90d"] as Period[]).map((p) => (
            <button
              key={p}
              onClick={() => setPeriod(p)}
              className={[
                "px-2.5 py-1 text-[10px] font-medium transition-colors rounded-[8px]",
                period === p
                  ? "bg-[rgba(242,184,75,0.12)] text-[var(--accent)]"
                  : "bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.35)] hover:bg-[rgba(255,255,255,0.08)]",
              ].join(" ")}
            >
              {t(("stats.period." + p) as any)}
            </button>
          ))}
        </div>
      </div>

      {/* Hero: the product's core value metric */}
      {today && (
        <div className="bg-[rgba(217,119,6,0.07)] border border-[rgba(217,119,6,0.18)] rounded-[14px] px-5 py-4.5 flex items-end justify-between shadow-[inset_0_1px_0_rgba(255,255,255,0.03)]">
          <div>
            <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.35)] mb-1">
              {t("stats.time-saved")} · {t(("stats.period." + period) as any)}
            </div>
            <div className="text-[30px] font-semibold tracking-[-0.03em] text-[var(--text-primary)] leading-none tabular-nums">
              {formatTimeSaved(agg.timeSaved)}
            </div>
          </div>
          <div className="text-right">
            <div className="text-[15px] font-medium text-[rgba(255,255,255,0.75)]">
              {formatTimeSaved(today.time_saved_secs)}
            </div>
            <div className="text-[9px] text-[rgba(255,255,255,0.3)]">{t("stats.today")}</div>
          </div>
        </div>
      )}

      {/* KPI tiles */}
      {kpis.length > 0 && (
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-2.5">
          {kpis.map((k) => (
            <div
              key={k.label}
              className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.075)] rounded-[12px] p-3.5 flex flex-col gap-1 min-w-0"
            >
              <span className="text-[10px] uppercase tracking-wider text-[var(--text-muted)] truncate">
                {k.label}
              </span>
              <span className="text-[19px] font-semibold tracking-[-0.02em] text-[var(--text-primary)] truncate tabular-nums">{k.value}</span>
              <span className="text-[10px] text-[var(--text-muted)] truncate">{k.sub}</span>
            </div>
          ))}
        </div>
      )}

      {/* Daily activity chart + metric switcher */}
      <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)] rounded-[10px] p-3.5">
        <div className="flex items-center justify-between mb-4">
          <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
            {metric === "words" ? t("stats.daily-words") : metric === "sessions" ? t("stats.daily-sessions") : t("stats.daily-time-saved")}
          </span>
          <div className="flex gap-1">
            {(
              [
                { id: "words", label: "stats.metric.words" },
                { id: "sessions", label: "stats.metric.sessions" },
                { id: "time", label: "stats.metric.time" },
              ] as { id: Metric; label: string }[]
            ).map((m) => (
              <button
                key={m.id}
                onClick={() => setMetric(m.id)}
                className={[
                  "px-2 py-0.5 text-[9px] font-medium rounded-md transition-colors",
                  metric === m.id
                    ? "bg-[rgba(242,184,75,0.15)] text-[var(--accent)]"
                    : "text-[rgba(255,255,255,0.3)] hover:text-[rgba(255,255,255,0.5)]",
                ].join(" ")}
              >
                {t(m.label as any)}
              </button>
            ))}
          </div>
        </div>
        {loading ? (
          <div className="h-[130px] flex items-center justify-center">
            <span className="text-[rgba(255,255,255,0.2)] text-[11px]">{t("stats.loading")}</span>
          </div>
        ) : (
          <BarChart stats={stats} metric={metric} />
        )}
      </div>

      {/* Dictation latency percentiles (issue #4) */}
      {latency && latency.count > 0 && (
        <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)] rounded-[10px] p-3.5">
          <div className="flex items-center justify-between mb-3">
            <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
              {t("stats.dictation-latency")} · {t(("stats.period." + period) as any)}
            </span>
            <span className="text-[9px] text-[rgba(255,255,255,0.25)]">
              {latency.count} {t("stats.unit.dictations")} · {t("stats.latency-desc")}
            </span>
          </div>
          <div className="grid grid-cols-4 gap-2 mb-3">
            {[
              { label: "P50", value: `${latency.p50_ms.toLocaleString()}ms`, hero: true },
              { label: "P95", value: `${latency.p95_ms.toLocaleString()}ms`, hero: true },
              { label: t("stats.avg"), value: `${latency.avg_ms.toLocaleString()}ms`, hero: false },
              { label: t("stats.fastest"), value: `${latency.min_ms.toLocaleString()}ms`, hero: false },
            ].map((tile) => (
              <div key={tile.label} className="flex flex-col gap-0.5">
                <span className="text-[9px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">{tile.label}</span>
                <span className={tile.hero ? "text-[20px] font-semibold text-[#fafaf9]" : "text-[15px] font-medium text-[rgba(255,255,255,0.6)]"}>
                  {tile.value}
                </span>
              </div>
            ))}
          </div>
          {latency.by_model.length > 0 && (
            <div className="flex flex-col gap-1 border-t border-[rgba(255,255,255,0.04)] pt-2.5">
              {latency.by_model.map((m) => (
                <div key={m.model} className="flex items-center gap-2 text-[10px]">
                  <span className="text-[rgba(255,255,255,0.55)] truncate flex-1">{m.model || t("stats.unknown-model")}</span>
                  <span className="text-[rgba(255,255,255,0.35)] font-mono">P50 {m.p50_ms.toLocaleString()}ms</span>
                  <span className="text-[rgba(255,255,255,0.25)] font-mono">P95 {m.p95_ms.toLocaleString()}ms</span>
                  <span className="text-[rgba(255,255,255,0.2)] w-10 text-right">{m.count}×</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Activity mix */}
      <ActivityMix stats={stats} />
    </div>
  );
}
