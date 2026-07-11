// 使用总览：自动生成的只读速查表（触发方式 → 配方），含 pill 滚轮一览与
// 期一遗留快捷键（agent/meeting/sts，标注「遗留」）。视图不是数据。
import { t, useT } from "../../lib/i18n";
import { workflowLabel } from "../../lib/builtinLabels";
import { pillWorkflows } from "../../lib/triggers";
import type { AppConfig, WorkflowRow } from "../../types";

export default function UsageOverview({
  rows, config, onJump, showTitle = true,
}: {
  rows: WorkflowRow[];
  config: AppConfig;
  onJump: (id: string) => void;
  showTitle?: boolean;
}) {
  useT();
  const kbd = "font-mono text-[10.5px] text-[var(--text-primary,#f5f3ef)] bg-[rgba(255,255,255,0.05)] border border-[rgba(255,255,255,0.075)] rounded-[5px] px-1.5 py-px";
  const hotkeyRows = rows.flatMap((w) =>
    (w.triggers ?? []).flatMap((tr) =>
      tr.kind === "hotkey"
        ? [{ combo: tr.combo, capture: tr.capture, wf: w }]
        : [],
    ),
  );
  const pill = pillWorkflows(rows);
  const legacy: { combo?: string; label: string }[] = [
    { combo: config.hotkey_agent, label: "Agent" },
    { combo: config.hotkey_meeting, label: "Meeting" },
    { combo: config.hotkey_sts, label: "STS" },
  ].filter((x) => !!x.combo);
  return (
    <div className="mb-3.5 rounded-[12px] border border-[rgba(255,255,255,0.075)] bg-[rgba(255,255,255,0.02)] p-4">
      {showTitle && <div className="text-[12px] font-semibold">{t("wb.overview.title")}</div>}
      <div className="text-[10.5px] text-[rgba(255,255,255,0.28)] mb-2.5">{t("wb.overview.note")}</div>
      <table className="w-full border-collapse text-[11px]">
        <tbody>
          {hotkeyRows.map((r, i) => (
            <tr key={i} className="border-t border-[rgba(255,255,255,0.045)] first:border-t-0">
              <td className="py-1.5 pr-2 w-[220px] text-[rgba(255,255,255,0.62)]">
                <span className={kbd}>{r.combo}</span>
                {r.wf.source_type_tag === "microphone" && (
                  <span className="text-[rgba(255,255,255,0.28)]"> · {t(r.capture === "toggle" ? "widgets.field.capture.toggle" : "widgets.field.capture.hold")}</span>
                )}
                {r.wf.source_type_tag === "selection" && (
                  <span className="text-[rgba(255,255,255,0.28)]"> · {t("wb.triggers.selection")}</span>
                )}
              </td>
              <td className="w-[30px] text-center text-[rgba(255,255,255,0.28)]">→</td>
              <td>
                <a onClick={() => onJump(r.wf.id)}
                   className="cursor-pointer border-b border-dotted border-[rgba(255,255,255,0.25)] hover:text-[var(--accent)]">
                  {workflowLabel(r.wf)}
                </a>
              </td>
            </tr>
          ))}
          {pill.length > 0 && (
            <tr className="border-t border-[rgba(255,255,255,0.045)]">
              <td className="py-1.5 pr-2 text-[rgba(255,255,255,0.62)]"><span className={kbd}>◉ {t("wb.triggers.pill-roller")}</span></td>
              <td className="w-[30px] text-center text-[rgba(255,255,255,0.28)]">→</td>
              <td>
                {pill.map((w, i) => (
                  <span key={w.id}>
                    {i > 0 && <span className="text-[rgba(255,255,255,0.28)]"> · </span>}
                    <a onClick={() => onJump(w.id)}
                       className="cursor-pointer border-b border-dotted border-[rgba(255,255,255,0.25)] hover:text-[var(--accent)]">
                      {workflowLabel(w)}
                    </a>
                  </span>
                ))}
              </td>
            </tr>
          )}
          {legacy.map((l, i) => (
            <tr key={`legacy-${i}`} className="border-t border-[rgba(255,255,255,0.045)] text-[rgba(255,255,255,0.28)]">
              <td className="py-1.5 pr-2"><span className={kbd}>{l.combo}</span></td>
              <td className="w-[30px] text-center">→</td>
              <td>
                {l.label}
                <span className="ml-1.5 rounded-[5px] border border-[rgba(255,255,255,0.09)] px-1.5 text-[9px]">{t("wb.overview.legacy")}</span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
