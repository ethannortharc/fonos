// Pure resolution logic for the notebook widget's name-is-identity combobox
// (2026-07-13 notebook unification spec §1): the user's typed/picked input
// maps to either an existing container id or a pending title to create at
// save time. UI-free so it unit-tests without the DOM.

import type { Container } from "./storage-api";

export type NotebookSelection =
  | { kind: "existing"; container_id: number }
  | { kind: "create"; title: string };

/** Empty input ⇒ Quick Note sentinel (0). A trimmed exact title match binds
 *  the existing notebook (no case folding, no fuzzy match); any other text
 *  is a pending create. */
export function resolveNotebookInput(
  input: string,
  notebooks: Container[]
): NotebookSelection {
  const title = input.trim();
  if (!title) return { kind: "existing", container_id: 0 };
  const match = notebooks.find(
    (c) => c.container_type === "notebook" && c.title === title
  );
  if (match) return { kind: "existing", container_id: match.id };
  return { kind: "create", title };
}
