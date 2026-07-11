// Vocabulary — top-level page (P1 IA). Thin wrapper around the existing
// Settings › Vocab tab body, now promoted out of Settings.
import VocabTab from "./settings/VocabTab";
import { useT } from "../lib/i18n";
import { useAppConfig } from "../lib/useAppConfig";

export default function VocabPage() {
  const t = useT();
  const { config, save } = useAppConfig();
  if (!config) return null;
  return (
    <div className="h-full flex flex-col">
      <div className="px-[26px] pt-5 flex-shrink-0">
        <div className="fonos-eyebrow">VOCABULARY</div>
        <h1 className="fonos-page-title mt-[3px]">{t("nav.vocab")}</h1>
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto px-[26px] py-4">
        <VocabTab config={config} onSave={save} />
      </div>
    </div>
  );
}
