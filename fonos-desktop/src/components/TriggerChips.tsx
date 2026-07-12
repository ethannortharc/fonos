// 触发器芯片区：一个配方 0..n 枚芯片。快捷键芯片可带 capture（仅 mic 配方），
// pill 位芯片仅 mic 配方可挂。即改即存（onChange 上抛整个 triggers 数组）。
//
// `readOnly` (Task 16): renders the same chips with no editing affordances —
// no HotkeyInput/select/✕/＋. Used by RecipesSection's recipe cards now that
// trigger editing lives on the Home page; HomePage renders the editable form
// (readOnly unset/false).
import { useState } from "react";
import { HotkeyInput } from "./HotkeyInput";
import { t, useT } from "../lib/i18n";
import type { Trigger, WorkflowRow } from "../types";

type TriggerChipsProps = {
  wf: WorkflowRow;
  isMic: boolean;
  /** `order` to assign a newly-added pill-slot chip. Callers with visibility
   *  across workflows should pass a value past the highest existing pill
   *  `order` (cross-workflow collisions are otherwise possible since this
   *  component only sees its own workflow's triggers); defaults to 1000. */
  nextPillOrder?: number;
} & (
  | {
      /** Read-only: chips render as static badges, no add/remove/edit. */
      readOnly: true;
      onChange?: undefined;
    }
  | {
      readOnly?: false;
      /** Called with the full next `triggers` array on every edit. */
      onChange: (triggers: Trigger[]) => void;
    }
);

export default function TriggerChips({
  wf,
  isMic,
  onChange,
  nextPillOrder = 1000,
  readOnly = false,
}: TriggerChipsProps) {
  useT();
  const [adding, setAdding] = useState(false);
  const triggers = wf.triggers ?? [];
  const set = (i: number, next: Trigger) => onChange?.(triggers.map((x, j) => (j === i ? next : x)));
  const remove = (i: number) => onChange?.(triggers.filter((_, j) => j !== i));
  const hasPill = triggers.some((x) => x.kind === "pill_slot");

  return (
    <div className="flex items-center gap-1.5 flex-wrap">
      <span className="text-[10px] text-[rgba(255,255,255,0.28)] mr-0.5">{t("wb.triggers.label")}</span>
      {triggers.map((tr, i) =>
        tr.kind === "hotkey" ? (
          <span key={i} className="inline-flex items-center gap-1.5 rounded-[7px] border border-[rgba(255,255,255,0.09)] bg-[rgba(255,255,255,0.04)] px-2 py-[3px] text-[10.5px]">
            {readOnly ? (
              <span className="font-mono text-[rgba(255,255,255,0.62)]">{tr.combo}</span>
            ) : (
              <HotkeyInput value={tr.combo} onChange={(combo) => set(i, { ...tr, combo })} />
            )}
            {isMic && (
              readOnly ? (
                <span className="text-[10px] text-[rgba(255,255,255,0.43)]">
                  {t(tr.capture === "toggle" ? "widgets.field.capture.toggle" : "widgets.field.capture.hold")}
                </span>
              ) : (
                <select
                  value={tr.capture ?? "hold"}
                  onChange={(e) => set(i, { ...tr, capture: e.target.value as "hold" | "toggle" })}
                  className="bg-transparent text-[10px] text-[rgba(255,255,255,0.43)] outline-none"
                >
                  <option value="hold">{t("widgets.field.capture.hold")}</option>
                  <option value="toggle">{t("widgets.field.capture.toggle")}</option>
                </select>
              )
            )}
            {!isMic && wf.source_type_tag === "selection" && (
              <span className="text-[10px] text-[rgba(255,255,255,0.28)]">· {t("wb.triggers.selection")}</span>
            )}
            {!readOnly && (
              <button onClick={() => remove(i)} aria-label={t("wb.triggers.remove")}
                className="text-[rgba(255,255,255,0.28)] hover:text-[#ef4444]">✕</button>
            )}
          </span>
        ) : (
          <span key={i} className="inline-flex items-center gap-1.5 rounded-[7px] border border-[rgba(240,173,50,0.28)] bg-[rgba(240,173,50,0.07)] px-2 py-[3px] text-[10.5px] text-[var(--accent)]">
            ◉ {t("wb.triggers.pill")}
            {!readOnly && (
              <button onClick={() => remove(i)} aria-label={t("wb.triggers.remove")}
                className="text-[rgba(255,255,255,0.28)] hover:text-[#ef4444]">✕</button>
            )}
          </span>
        ),
      )}
      {!readOnly && (adding ? (
        <span className="inline-flex items-center gap-1.5">
          <HotkeyInput
            value=""
            placeholder={t("wb.triggers.press-keys")}
            onChange={(combo) => {
              if (combo) onChange?.([...triggers, { kind: "hotkey", combo, ...(isMic ? { capture: "hold" as const } : {}) }]);
              setAdding(false);
            }}
          />
          {isMic && !hasPill && (
            <button
              onClick={() => { onChange?.([...triggers, { kind: "pill_slot", order: nextPillOrder }]); setAdding(false); }}
              className="rounded-[7px] border border-[rgba(240,173,50,0.2)] px-2 py-[3px] text-[10.5px] text-[rgba(242,184,75,0.85)]"
            >
              ◉ {t("wb.triggers.pill")}
            </button>
          )}
          <button onClick={() => setAdding(false)} className="text-[10.5px] text-[rgba(255,255,255,0.28)]">{t("common.cancel")}</button>
        </span>
      ) : (
        <button
          onClick={() => setAdding(true)}
          className="rounded-[7px] border border-dashed border-[rgba(255,255,255,0.12)] px-2 py-[3px] text-[10.5px] text-[rgba(255,255,255,0.28)] hover:text-[var(--accent)] hover:border-[rgba(242,184,75,0.3)]"
        >
          {t("wb.triggers.add")}
        </button>
      ))}
    </div>
  );
}
