// Node-capsule pipeline renderer (Flows UI redesign, Task 2) — draws a
// source → processors → outputs chain as role-colored capsules joined by
// muted SVG arrows. Two modes:
//  - read-only (`interactive` falsy): pure display, used for flow-list row
//    previews (replaces WorkflowsTab's text-only PipelineSummary).
//  - interactive: capsules are clickable (drives in-place node editing),
//    the `activeId` node gets an outline, and — only in gaps between two
//    processor nodes — a small "+" button offers mid-chain insertion via
//    `onAddStep(afterIndex)`. Inserting before the first processor or after
//    the last one is intentionally left to a different affordance in the
//    caller (e.g. an explicit "append step" button), matching the existing
//    WorkflowsTab pattern.

import { t, useT } from "../lib/i18n";
import type { WidgetRole } from "../types";
import { WidgetIcon, roleColor } from "./WidgetIcon";

export interface PipeNode {
  id: string;
  role: WidgetRole;
  typeTag: string;
  label: string;
}

// ─── Muted connector arrow ──────────────────────────────────────────────────

function Arrow() {
  return (
    <svg width="18" height="14" viewBox="0 0 18 14" className="flex-shrink-0" style={{ color: "rgba(255,255,255,0.18)" }}>
      <line x1="0" y1="7" x2="12" y2="7" stroke="currentColor" strokeWidth="1.5" />
      <polyline points="9,2 15,7 9,12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

// ─── "+" mid-chain insert button ────────────────────────────────────────────

function AddStepButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      title={t("wf.add-step")}
      aria-label={t("wf.add-step")}
      className="flex-shrink-0 w-4 h-4 flex items-center justify-center rounded-full border border-dashed border-[rgba(245,158,11,0.35)] text-[rgba(251,191,36,0.7)] text-[11px] leading-none hover:text-[#fbbf24] hover:border-[rgba(245,158,11,0.6)] transition-colors"
    >
      +
    </button>
  );
}

// ─── Node capsule ────────────────────────────────────────────────────────────

function NodeCapsule({
  node, interactive, active, onClick,
}: {
  node: PipeNode;
  interactive: boolean;
  active: boolean;
  onClick?: () => void;
}) {
  const rc = roleColor(node.role);
  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (!interactive) return;
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onClick?.(); }
  };
  return (
    <div
      role={interactive ? "button" : undefined}
      tabIndex={interactive ? 0 : undefined}
      onClick={interactive ? onClick : undefined}
      onKeyDown={handleKeyDown}
      title={node.label}
      className={[
        "flex items-center gap-1.5 flex-shrink-0",
        interactive ? "cursor-pointer hover:brightness-125 transition-[filter]" : "",
      ].join(" ")}
      style={{
        background: `rgba(${rc.rgb},0.08)`,
        border: "1px solid " + `rgba(${rc.rgb},0.22)`,
        color: `rgba(${rc.rgb},0.95)`,
        borderRadius: 9,
        padding: "6px 11px",
        outline: active ? `2px solid rgba(${rc.rgb},0.55)` : "none",
        outlineOffset: 2,
      }}
    >
      <WidgetIcon typeTag={node.typeTag} size={13} />
      <span className="text-[11px] font-medium whitespace-nowrap">{node.label}</span>
    </div>
  );
}

// ─── Main PipelineView ──────────────────────────────────────────────────────

export default function PipelineView({
  nodes, interactive, activeId, onNodeClick, onAddStep,
}: {
  nodes: PipeNode[];
  interactive?: boolean;
  activeId?: string;
  onNodeClick?: (id: string) => void;
  onAddStep?: (afterIndex: number) => void;
}) {
  useT();
  return (
    <div className="flex items-center flex-wrap gap-1">
      {nodes.map((n, i) => {
        // Mid-chain insertion only between two processor nodes — inserting
        // before the first / after the last processor is a different
        // affordance (see file header).
        const showAdd =
          !!interactive && !!onAddStep && i > 0 &&
          nodes[i - 1].role === "processor" && n.role === "processor";
        return (
          <div key={n.id} className="flex items-center gap-1">
            {i > 0 && (
              <span className="flex items-center gap-0.5">
                <Arrow />
                {showAdd && <AddStepButton onClick={() => onAddStep!(i - 1)} />}
              </span>
            )}
            <NodeCapsule
              node={n}
              interactive={!!interactive}
              active={!!interactive && activeId === n.id}
              onClick={() => onNodeClick?.(n.id)}
            />
          </div>
        );
      })}
    </div>
  );
}
