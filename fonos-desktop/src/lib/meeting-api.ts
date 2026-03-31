// Typed Tauri IPC wrappers for meeting commands.
// Uses @tauri-apps/api v2 — no raw __TAURI_INTERNALS__ calls.

import { invoke } from "@tauri-apps/api/core";
import type { Entry, Container } from "./storage-api";

export interface MeetingDetail {
  container: Container;
  entries: Entry[];
  summary: Entry | null;
}

export async function startMeeting(): Promise<number> {
  return invoke<number>("start_meeting");
}

export async function stopMeeting(): Promise<string> {
  return invoke<string>("stop_meeting");
}

export async function getMeetings(): Promise<Container[]> {
  return invoke<Container[]>("get_meetings");
}

export async function getMeetingDetail(containerId: number): Promise<MeetingDetail> {
  return invoke<MeetingDetail>("get_meeting_detail", { container_id: containerId });
}

export async function exportMeetingMd(containerId: number, outputDir: string): Promise<string> {
  return invoke<string>("export_meeting_md", { container_id: containerId, output_dir: outputDir });
}

export async function exportMeetingJson(containerId: number, outputDir: string): Promise<string> {
  return invoke<string>("export_meeting_json", { container_id: containerId, output_dir: outputDir });
}
