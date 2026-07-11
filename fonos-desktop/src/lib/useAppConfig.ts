// Shared config-loading hook for top-level pages (Workbench, Models, Vocab).
// Mirrors the load + handleSave pattern Settings.tsx used to own directly:
// `getConfig()` already returns a parsed AppConfig (no JSON.parse needed —
// only `saveConfig` takes a JSON string, for the merge-and-persist side).

import { useCallback, useEffect, useState } from "react";
import { getConfig, saveConfig } from "./api";
import type { AppConfig } from "../types";

/** Load-once config state with a JSON-merge save, shared by top-level pages. */
export function useAppConfig() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const reload = useCallback(async () => {
    setConfig(await getConfig());
  }, []);
  useEffect(() => {
    void reload();
  }, [reload]);
  const save = useCallback(async (updates: Partial<AppConfig>) => {
    await saveConfig(JSON.stringify(updates));
    setConfig((c) => (c ? { ...c, ...updates } : c));
  }, []);
  return { config, save, reload };
}
