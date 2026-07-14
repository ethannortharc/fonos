// WidgetsSection.tsx — the Workbench's 组件 (Widgets) segment: type shelves
// (Sources/Processors/Delivery/Sessions — GROUPS in typeMeta.ts, each broken
// into its type_tags) with instance cards per configured widget (props
// summary + used-by-N-recipes count + Test ▶ jump + expand-to-edit via
// WidgetForm). Delivery/Sessions are both role "output" at the model layer
// (see typeMeta.ts's GROUPS comment) — the split is presentation-only, so
// tinting/creation still key off `role`, not `key`. Types are the shelves;
// widgets are the tuned, named instances that sit on them — each instance
// can be edited or test-run on its own. Supersedes BuildingBlocks.tsx's
// read-only type catalog (Task 9): shelves are now the SAME surface where
// instances live, not a separate informational tab.

import { useCallback, useEffect, useState } from "react";
import { listWidgets, listWorkflows, saveWidget, deleteWidget } from "../../lib/api";
import { useT, t } from "../../lib/i18n";
import { widgetLabel } from "../../lib/builtinLabels";
import { usageCount } from "../../lib/triggers";
import { WidgetIcon, roleColor } from "../../components/WidgetIcon";
import WidgetForm, { widgetToForm } from "../settings/WidgetForm";
import { GROUPS, TYPE_META } from "./typeMeta";
import type { Container } from "../../lib/storage-api";
import type { AppConfig, WidgetDef, WorkflowRow } from "../../types";

const msg = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** props 摘要：挑几项关键 prop 拼成一行（模型/温度/语言等）。 */
export function summarizeProps(w: WidgetDef): string {
  const p = (w.props ?? {}) as Record<string, unknown>;
  const parts: string[] = [];
  for (const k of ["model_profile", "language", "temperature", "strategy", "container_id", "voice"]) {
    const v = p[k];
    if (v !== undefined && v !== null && v !== "") parts.push(String(v));
  }
  return parts.join(" · ");
}

export default function WidgetsSection({
  config,
  containers,
  onContainerCreated,
  onTest,
}: {
  config: AppConfig;
  containers: Container[];
  /** Reload the owner's (Workbench's) containers list after a notebook widget
   *  mints a new container at save time — forwarded to WidgetForm so
   *  name-is-identity holds across this stale-once-loaded list. */
  onContainerCreated?: () => void;
  onTest: (widgetId: string) => void;
}) {
  useT();
  const [widgets, setWidgets] = useState<WidgetDef[]>([]);
  const [rows, setRows] = useState<WorkflowRow[]>([]);
  const [editing, setEditing] = useState<string | null>(null); // widget id
  const [creating, setCreating] = useState<string | null>(null); // type_tag
  const [deleteError, setDeleteError] = useState<string>("");
  const load = useCallback(async () => {
    try {
      setWidgets(await listWidgets());
      setRows(await listWorkflows());
    } catch (e) {
      console.error("list_widgets/list_workflows:", e);
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);

  const onSaved = async (w: WidgetDef) => {
    await saveWidget(w);
    setEditing(null);
    setCreating(null);
    await load();
  };

  const onDeleteWidget = async (id: string) => {
    setDeleteError("");
    try {
      await deleteWidget(id);
      setEditing(null);
      await load();
    } catch (e) {
      setDeleteError(msg(e));
    }
  };

  return (
    <div>
      {GROUPS.map(({ key, role, label, tags }) => {
        const rc = roleColor(role);
        return (
          <div key={key} className="mb-[22px]">
            <div className="flex items-center gap-2 mb-2">
              <span className="text-[10px] uppercase tracking-wider font-semibold" style={{ color: `rgba(${rc.rgb},0.75)` }}>
                {t(label)}
              </span>
            </div>
            {tags.map((tag) => {
              const meta = TYPE_META[tag];
              const instances = widgets.filter((w) => w.type_tag === tag);
              return (
                <div key={tag} className="mb-4">
                  <div className="flex items-baseline gap-2.5 pb-2 mb-2.5 border-b border-[rgba(255,255,255,0.05)]">
                    <span className="flex items-center gap-[7px] text-[12.5px] font-semibold" style={{ color: `rgba(${rc.rgb},0.95)` }}>
                      <WidgetIcon typeTag={tag} size={15} />
                      <span className="text-[var(--text-primary,#f5f3ef)]">{meta ? t(meta.name) : tag}</span>
                    </span>
                    <span className="flex-1 text-[10.5px] text-[rgba(255,255,255,0.43)]">{meta ? t(meta.desc) : ""}</span>
                    <button
                      onClick={() => { setEditing(null); setDeleteError(""); setCreating(tag); }}
                      className="rounded-[7px] border border-dashed border-[rgba(242,184,75,0.16)] px-2.5 py-[3px] text-[10.5px] text-[rgba(242,184,75,0.6)] hover:text-[var(--accent)] hover:border-[rgba(242,184,75,0.34)] transition-colors"
                    >
                      {t("wb.widgets.new")}
                    </button>
                  </div>
                  {creating === tag && (
                    <div className="mb-2.5">
                      <WidgetForm
                        value={{ id: `${tag}.custom-${Date.now()}`, role, type_tag: tag, name: "", icon: "", props: {}, builtin: false, isNew: true }}
                        config={config}
                        containers={containers}
                        widgets={widgets}
                        typeTags={[tag]}
                        onSave={onSaved}
                        onCancel={() => setCreating(null)}
                        onContainerCreated={onContainerCreated}
                      />
                    </div>
                  )}
                  <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2.5">
                    {instances.map((w) => {
                      const used = usageCount(w.id, rows, widgets);
                      const open = editing === w.id;
                      return (
                        <div key={w.id} className="rounded-[12px] border border-[rgba(255,255,255,0.075)] bg-[rgba(255,255,255,0.02)] hover:border-[rgba(255,255,255,0.13)] transition-colors p-[13px]">
                          <div
                            className="flex items-center gap-2 cursor-pointer"
                            onClick={() => { setCreating(null); setDeleteError(""); setEditing(open ? null : w.id); }}
                          >
                            <span style={{ color: `rgba(${rc.rgb},0.9)` }}><WidgetIcon typeTag={w.type_tag} size={13} /></span>
                            <span className="flex-1 text-[12px] font-semibold truncate">{widgetLabel(w)}</span>
                          </div>
                          <div className="mt-[7px] text-[10.5px] leading-[1.5] text-[rgba(255,255,255,0.43)] truncate">
                            {summarizeProps(w) || t("widgets.no-config")}
                          </div>
                          <div className="mt-2 flex items-center gap-2">
                            <span className="flex-1 text-[10px] text-[rgba(255,255,255,0.28)]">
                              {t("wb.widgets.used-by").replace("{0}", String(used))}
                            </span>
                            <button
                              onClick={() => onTest(w.id)}
                              className="rounded-[7px] border border-[rgba(255,255,255,0.08)] bg-[rgba(255,255,255,0.025)] px-2.5 py-[3px] text-[10.5px] text-[rgba(255,255,255,0.43)] hover:text-[var(--accent)] hover:border-[rgba(242,184,75,0.3)] transition-colors"
                            >
                              {t("wb.widgets.test")}
                            </button>
                          </div>
                          {open && (
                            <div className="mt-2.5">
                              {used > 0 && (
                                <div className="mb-1.5 text-[10px] text-[rgba(242,184,75,0.8)]">
                                  {t("wb.widgets.share-warn").replace("{0}", String(used))}
                                </div>
                              )}
                              <WidgetForm
                                value={widgetToForm(w)}
                                config={config}
                                containers={containers}
                                widgets={widgets}
                                onSave={onSaved}
                                onCancel={() => setEditing(null)}
                                onDelete={w.builtin ? undefined : () => { void onDeleteWidget(w.id); }}
                                deleteError={open ? deleteError : undefined}
                                onContainerCreated={onContainerCreated}
                              />
                            </div>
                          )}
                        </div>
                      );
                    })}
                    {instances.length === 0 && (
                      <div className="text-[10.5px] text-[rgba(255,255,255,0.28)] py-2">{t("wb.widgets.empty")}</div>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        );
      })}
    </div>
  );
}
