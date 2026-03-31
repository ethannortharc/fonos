/**
 * Playwright global teardown: stops the Fonos API server started by global-setup.ts.
 */

import fs from "fs";
import path from "path";

const PID_FILE = path.join(__dirname, ".fonos-server.pid");

export default async function globalTeardown(): Promise<void> {
  if (!fs.existsSync(PID_FILE)) return;

  const pid = parseInt(fs.readFileSync(PID_FILE, "utf8").trim(), 10);
  fs.unlinkSync(PID_FILE);

  try {
    process.kill(pid, "SIGTERM");
    console.log(`[global-teardown] Sent SIGTERM to Fonos server PID ${pid}`);
  } catch (e) {
    console.warn(`[global-teardown] Could not kill PID ${pid}: ${e}`);
  }
}
