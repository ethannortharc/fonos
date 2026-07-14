// Models — top-level page (P1 IA). Thin wrapper around the existing
// Settings › Models tab body, now promoted out of Settings. The header hosts
// the unified guided "add engine" entry (onboarding P3) — the same Scenarios
// overlay reachable from Settings and first-run skip.
import { useState } from "react";
import ModelsTab from "./settings/ModelsTab";
import Scenarios from "./Scenarios";
import { useT } from "../lib/i18n";
import { useAppConfig } from "../lib/useAppConfig";

export default function ModelsPage() {
  const t = useT();
  const { config, save, reload } = useAppConfig();
  const [error, setError] = useState<string>("");
  const [showGuide, setShowGuide] = useState(false);
  if (!config) return null;
  return (
    <div className="h-full flex flex-col">
      {showGuide && (
        <Scenarios
          mode="overlay"
          onDone={() => {
            setShowGuide(false);
            void reload();
          }}
        />
      )}
      <div className="px-[26px] pt-5 flex-shrink-0 flex items-end justify-between gap-3">
        <div>
          <div className="fonos-eyebrow">MODELS</div>
          <h1 className="fonos-page-title mt-[3px]">{t("nav.models")}</h1>
        </div>
        <button
          data-testid="models-add-engine"
          onClick={() => setShowGuide(true)}
          className="text-[11px] px-3 py-1.5 rounded-md border border-[rgba(242,184,75,0.25)] bg-[rgba(242,184,75,0.08)] text-[var(--accent)] hover:bg-[rgba(242,184,75,0.14)] transition-colors"
        >
          {t("models.add-engine")}
        </button>
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto px-[26px] py-4">
        {error && (
          <div className="rounded-lg bg-red-500/10 border border-red-500/20 p-3 mb-3">
            <p className="text-red-400 text-xs">{error}</p>
          </div>
        )}
        <ModelsTab config={config} onSave={save} setError={setError} />
      </div>
    </div>
  );
}
