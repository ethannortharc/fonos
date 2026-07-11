// Models — top-level page (P1 IA). Thin wrapper around the existing
// Settings › Models tab body, now promoted out of Settings.
import { useState } from "react";
import ModelsTab from "./settings/ModelsTab";
import { useT } from "../lib/i18n";
import { useAppConfig } from "../lib/useAppConfig";

export default function ModelsPage() {
  const t = useT();
  const { config, save } = useAppConfig();
  const [error, setError] = useState<string>("");
  if (!config) return null;
  return (
    <div className="h-full flex flex-col">
      <div className="px-[26px] pt-5 flex-shrink-0">
        <div className="fonos-eyebrow">MODELS</div>
        <h1 className="fonos-page-title mt-[3px]">{t("nav.models")}</h1>
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
