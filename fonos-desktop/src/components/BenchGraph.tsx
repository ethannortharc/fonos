// BenchGraph — 试运行节点图：PipelineView 胶囊的放大版 + 每节点挂载荷卡（入/出/
// 耗时/拦截徽章）。纯展示组件，状态由 TestRunSection 驱动。
import { WidgetIcon, roleColor } from "./WidgetIcon";
import { t, useT } from "../lib/i18n";
import type { WidgetRole } from "../types";

export type BenchNode = {
  id: string;
  role: WidgetRole;
  typeTag: string;
  label: string;
  state: "idle" | "active" | "done" | "error";
  payload?: { preview?: string; ms?: number; error?: string; intercepted?: boolean };
};

export default function BenchGraph({
  nodes, onNodeClick,
}: { nodes: BenchNode[]; onNodeClick?: (id: string) => void }) {
  useT();
  return (
    // pt-1: `overflow-x-auto` (horizontal scroll for long chains) forces
    // overflow-y to compute to `auto` too — so this scroll container also clips
    // vertically, at its PADDING box. The active/error node ring is an OUTER
    // box-shadow (`0 0 0 2px …`, see the capsule below) that extends 2px beyond
    // the capsule border box; with the capsules flush at the container's top
    // (padding-top was 0, vs px-0.5/pb-2 elsewhere), that ring's top edge fell
    // outside the clip region and was shaved off on every node — bottom/left/
    // right survived on their existing padding. pt-1 (3px) extends the clip
    // region just enough to admit the full 2px ring on top; the ~3px it nudges
    // the graph down is imperceptible and it leaves the wrapper's opaque-fill
    // aura occluder (TestRunSection, 58ecdab) untouched. Pixel-verified on
    // first/middle/last nodes (see testrun-ring-fix-report.md).
    <div className={["flex items-start gap-2 overflow-x-auto px-0.5 pt-1 pb-2", nodes.length === 1 ? "single" : ""].join(" ")}>
      {nodes.map((n, i) => {
        const rc = roleColor(n.role);
        return (
          <div key={`${n.id}-${i}`} className="contents">
            {i > 0 && (
              <svg width="20" height="16" viewBox="0 0 18 14" className="mt-2.5 flex-shrink-0"
                   style={{ color: nodes[i].state !== "idle" ? "rgba(242,184,75,0.8)" : "rgba(255,255,255,0.32)" }}>
                <line x1="0" y1="7" x2="12" y2="7" stroke="currentColor" strokeWidth="1.5" />
                <polyline points="9,2 15,7 9,12" fill="none" stroke="currentColor" strokeWidth="1.5"
                          strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            )}
            <div className={nodes.length === 1 ? "min-w-[340px] max-w-[480px]" : "min-w-[168px] max-w-[235px] flex-shrink-0"}>
              <div
                role="button"
                tabIndex={0}
                onClick={() => onNodeClick?.(n.id)}
                className="flex items-center gap-[7px] rounded-[10px] px-[13px] py-[9px] text-[12px] font-semibold cursor-pointer transition-shadow hover:brightness-125"
                style={{
                  background: `rgba(${rc.rgb},0.08)`,
                  border: `1px solid rgba(${rc.rgb},0.22)`,
                  color: `rgba(${rc.rgb},0.95)`,
                  boxShadow: n.state === "active"
                    ? `0 0 0 2px rgba(${rc.rgb},0.5), 0 0 22px rgba(${rc.rgb},0.18)`
                    : n.state === "error"
                      ? "0 0 0 2px rgba(239,68,68,0.55)"
                      : "none",
                }}
              >
                <WidgetIcon typeTag={n.typeTag} size={14} />
                <span className="truncate">{n.label}</span>
                <span className="ml-auto font-normal" style={{ opacity: n.state === "done" ? 0.9 : 0 }}>✓</span>
              </div>
              <div className={[
                "mt-2 min-h-[64px] rounded-[9px] border px-2.5 py-2 text-[10.5px] leading-[1.55]",
                n.payload
                  ? "border-[rgba(255,255,255,0.09)] bg-[rgba(0,0,0,0.22)] text-[rgba(255,255,255,0.62)]"
                  : "border-[rgba(255,255,255,0.05)] bg-[rgba(0,0,0,0.22)] text-[rgba(255,255,255,0.28)]",
              ].join(" ")}>
                {!n.payload && t("wb.bench.awaiting")}
                {n.payload && (
                  <>
                    {n.payload.ms != null && (
                      <span className="float-right ml-1.5 text-[9.5px] text-[rgba(255,255,255,0.28)] tabular-nums">{n.payload.ms} ms</span>
                    )}
                    {n.payload.error ? (
                      <span className="text-[#ff8a75]">{n.payload.error}</span>
                    ) : (
                      <span className="block whitespace-pre-wrap break-words">{n.payload.preview}</span>
                    )}
                    {n.payload.intercepted && (
                      <span className="mt-1.5 inline-block rounded-[5px] border border-[rgba(240,173,50,0.25)] bg-[rgba(240,173,50,0.1)] px-1.5 text-[9.5px] font-semibold text-[var(--accent)]">
                        {t("wb.bench.intercepted")}
                      </span>
                    )}
                  </>
                )}
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
