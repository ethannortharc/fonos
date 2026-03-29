// Typed Tauri IPC wrappers for the v2 storage commands.
// Uses @tauri-apps/api v2 — no raw __TAURI_INTERNALS__ calls.

import { invoke } from "@tauri-apps/api/core";

// ─── Types matching Rust structs ──────────────────────────────────────────────

export type SourceType = "dictation" | "agent" | "note" | "meeting";
export type Role = "user" | "assistant" | "system";
export type ContainerTypeValue =
  | "notebook"
  | "conversation"
  | "meeting_session"
  | "journal"
  | "research";

/** Mirrors fonos_core::storage::Entry */
export interface Entry {
  id: number;
  created_at: string;
  source_type: SourceType;
  role: Role;
  mode: string;
  raw_text: string;
  processed_text: string;
  container_id: number | null;
  audio_ref: string | null;
  metadata: Record<string, unknown>;
}

/** Mirrors fonos_core::storage::Container */
export interface Container {
  id: number;
  container_type: ContainerTypeValue;
  title: string;
  parent_id: number | null;
  created_at: string;
  updated_at: string;
  metadata: Record<string, unknown>;
}

// ─── Entry commands ───────────────────────────────────────────────────────────

/** Fetch recent entries.  Optionally filter by source_type, limit, and offset. */
export async function listEntries(
  limit?: number,
  offset?: number,
  sourceType?: string
): Promise<Entry[]> {
  return invoke<Entry[]>("list_entries", {
    limit: limit ?? null,
    offset: offset ?? null,
    source_type: sourceType ?? null,
  });
}

/** Fetch a single entry by its row ID. */
export async function getEntry(id: number): Promise<Entry> {
  return invoke<Entry>("get_entry", { id });
}

/** Full-text search over all entries. */
export async function searchEntries(
  query: string,
  limit?: number
): Promise<Entry[]> {
  return invoke<Entry[]>("search_entries", {
    query,
    limit: limit ?? null,
  });
}

/** Update the text of an existing entry. */
export async function updateEntry(
  id: number,
  processedText: string
): Promise<void> {
  return invoke<void>("update_entry", { id, text: processedText });
}

/** Delete a container by its row ID. */
export async function deleteContainer(id: number): Promise<void> {
  return invoke<void>("delete_container", { id });
}

/** Delete an entry by its row ID. */
export async function deleteEntry(id: number): Promise<void> {
  return invoke<void>("delete_entry", { id });
}

// ─── Container commands ───────────────────────────────────────────────────────

/** Create a new container (notebook by default). */
export async function createContainer(
  title: string,
  containerType?: string
): Promise<Container> {
  return invoke<Container>("create_container", {
    title,
    container_type: containerType ?? null,
  });
}

/** List all containers. */
export async function listContainers(): Promise<Container[]> {
  return invoke<Container[]>("list_containers");
}

/** Fetch entries belonging to a specific container (chronological). */
export async function getContainerEntries(
  containerId: number
): Promise<Entry[]> {
  return invoke<Entry[]>("get_container_entries", { container_id: containerId });
}

// ─── Export commands ───────────────────────────────────────────────────────────

/** Export a notebook as a Markdown folder. Returns the output directory path. */
export async function exportNotebookMd(
  containerId: number,
  outputDir: string
): Promise<string> {
  return invoke<string>("export_notebook_md", { container_id: containerId, output_dir: outputDir });
}

/** Export a notebook as JSON. Returns the output file path. */
export async function exportNotebookJson(
  containerId: number,
  outputDir: string
): Promise<string> {
  return invoke<string>("export_notebook_json", { container_id: containerId, output_dir: outputDir });
}
