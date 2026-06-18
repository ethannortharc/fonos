import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import App from "./App";

const rootEl = document.getElementById("root") as HTMLElement | null;
if (!rootEl) throw new Error("Root element not found");
const appRoot = rootEl;

async function boot() {
  const readmeDemo =
    import.meta.env.DEV &&
    new URLSearchParams(window.location.search).get("demo") === "readme";

  if (readmeDemo) {
    const { installDemoIpc } = await import("./demo-ipc");
    installDemoIpc();
  }

  createRoot(appRoot).render(
    <StrictMode>
      <App />
    </StrictMode>
  );
}

boot();
