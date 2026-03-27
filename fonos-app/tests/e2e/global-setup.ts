/**
 * Playwright global setup: starts the Fonos Python API server before tests run.
 * The server process handle is stored so global-teardown.ts can stop it.
 */

import { spawn, ChildProcess } from "child_process";
import path from "path";
import fs from "fs";

const FONOS_WORKSPACE = path.resolve(__dirname, "../../../../");
const SERVER_PORT = 9880;
const HEALTH_URL = `http://127.0.0.1:${SERVER_PORT}/v1/health`;
const HEALTH_TIMEOUT_MS = 120_000;
const POLL_INTERVAL_MS = 1_000;
const PID_FILE = path.join(__dirname, ".fonos-server.pid");

async function waitForHealth(url: string, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {
      // server not ready yet
    }
    await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
  }
  throw new Error(
    `Fonos server did not become healthy at ${url} within ${timeoutMs}ms`
  );
}

export default async function globalSetup(): Promise<void> {
  if (!fs.existsSync(FONOS_WORKSPACE)) {
    console.warn(
      `[global-setup] SKIP: fonos workspace not found at ${FONOS_WORKSPACE}`
    );
    return;
  }

  const server: ChildProcess = spawn(
    "uv",
    [
      "run",
      "uvicorn",
      "fonos_service.server:app",
      "--host",
      "127.0.0.1",
      "--port",
      String(SERVER_PORT),
    ],
    {
      cwd: FONOS_WORKSPACE,
      stdio: "ignore",
      detached: false,
    }
  );

  if (!server.pid) {
    throw new Error("[global-setup] Failed to spawn Fonos server");
  }

  fs.writeFileSync(PID_FILE, String(server.pid), "utf8");
  console.log(`[global-setup] Fonos server PID ${server.pid} on port ${SERVER_PORT}`);

  await waitForHealth(HEALTH_URL, HEALTH_TIMEOUT_MS);
  console.log("[global-setup] Fonos server is healthy");
}
