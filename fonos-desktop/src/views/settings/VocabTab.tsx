// Vocabulary settings — user-defined vocab books (terms + correction rules).
// Books are referenced by id from global_vocab_books and per-mode vocab_books.
// Terms are edited as chips (type + Enter to add); rules as find→replace rows.

import { useRef, useState } from "react";
import type { AppConfig, VocabBook, VocabRule } from "../../types";

const EMPTY_RULE: VocabRule = { from: "", to: "", kind: "literal", case_insensitive: true };

function newBook(): VocabBook {
  return {
    id: `book-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`,
    name: "New book",
    enabled: true,
    terms: [],
    rules: [],
  };
}

const input =
  "bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2.5 py-1.5 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]";

/** Chip-style term editor: terms render as removable pills inside an
 *  input-looking container; typing + Enter (or comma / blur) adds, Backspace
 *  on an empty draft removes the last pill. Pasted comma/newline lists split
 *  into individual terms. */
function TermChips({ terms, onChange }: { terms: string[]; onChange: (t: string[]) => void }) {
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  const commit = (raw: string) => {
    const parts = raw
      .split(/[\n,，、;；]+/)
      .map((t) => t.trim())
      .filter(Boolean);
    if (parts.length === 0) return;
    const merged = [...terms];
    for (const p of parts) if (!merged.includes(p)) merged.push(p);
    onChange(merged);
    setDraft("");
  };

  const removeAt = (i: number) => onChange(terms.filter((_, idx) => idx !== i));

  return (
    <div
      onClick={() => inputRef.current?.focus()}
      className="min-h-[64px] flex flex-wrap items-start content-start gap-1.5 p-2 rounded-lg cursor-text bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] focus-within:border-[rgba(245,158,11,0.3)] transition-colors"
    >
      {terms.map((term, i) => (
        <span
          key={`${term}-${i}`}
          className="group inline-flex items-center gap-1 pl-2 pr-1 py-[3px] rounded-md bg-[rgba(251,191,36,0.07)] border border-[rgba(251,191,36,0.12)] text-[10.5px] text-[#fde68a] leading-none select-none"
        >
          {term}
          <button
            onClick={(e) => {
              e.stopPropagation();
              removeAt(i);
            }}
            tabIndex={-1}
            className="w-3.5 h-3.5 rounded flex items-center justify-center text-[10px] text-[rgba(253,230,138,0.35)] hover:text-[#fbbf24] hover:bg-[rgba(251,191,36,0.15)] transition-colors"
          >
            ×
          </button>
        </span>
      ))}
      <input
        ref={inputRef}
        type="text"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === "," || e.key === "，" || e.key === "、") {
            e.preventDefault();
            commit(draft);
          } else if (e.key === "Backspace" && draft === "" && terms.length > 0) {
            removeAt(terms.length - 1);
          }
        }}
        onBlur={() => commit(draft)}
        placeholder={terms.length === 0 ? "Type a term, press Enter — e.g. Kubernetes, OMLX…" : ""}
        spellCheck={false}
        className="flex-1 min-w-[110px] bg-transparent text-[11px] text-[#fafaf9] placeholder-[rgba(255,255,255,0.18)] outline-none py-[3px]"
      />
    </div>
  );
}

/** One find→replace rule row with kind / case toggles; delete reveals on hover. */
function RuleRow({
  rule,
  bookId,
  index,
  onPatch,
  onDelete,
}: {
  rule: VocabRule;
  bookId: string;
  index: number;
  onPatch: (patch: Partial<VocabRule>) => void;
  onDelete: () => void;
}) {
  const isRegex = rule.kind === "regex";
  return (
    <div className="group flex items-center gap-1.5 px-1.5 py-1 -mx-1.5 rounded-lg hover:bg-[rgba(255,255,255,0.02)] transition-colors">
      <input
        key={`from-${bookId}-${index}`}
        type="text"
        defaultValue={rule.from}
        onBlur={(e) => {
          if (e.target.value !== rule.from) onPatch({ from: e.target.value });
        }}
        placeholder={isRegex ? "pattern" : "heard as…"}
        spellCheck={false}
        className={`${input} flex-1 min-w-0 ${isRegex ? "font-mono" : ""}`}
      />
      <span className="w-5 h-5 rounded-full bg-[rgba(251,191,36,0.08)] text-[#fbbf24] text-[10px] flex items-center justify-center shrink-0">
        →
      </span>
      <input
        key={`to-${bookId}-${index}`}
        type="text"
        defaultValue={rule.to}
        onBlur={(e) => {
          if (e.target.value !== rule.to) onPatch({ to: e.target.value });
        }}
        placeholder="should be…"
        spellCheck={false}
        className={`${input} flex-1 min-w-0`}
      />
      <div className="flex rounded-lg overflow-hidden border border-[rgba(255,255,255,0.06)] shrink-0">
        <button
          onClick={() => onPatch({ kind: isRegex ? "literal" : "regex" })}
          title={isRegex ? "Regex pattern" : "Literal text (click for regex)"}
          className={[
            "px-2 py-1.5 text-[9px] font-mono transition-colors",
            isRegex
              ? "bg-[rgba(192,132,252,0.15)] text-[#c084fc]"
              : "bg-transparent text-[rgba(255,255,255,0.3)] hover:text-[rgba(255,255,255,0.6)]",
          ].join(" ")}
        >
          .*
        </button>
        <button
          onClick={() => onPatch({ case_insensitive: !rule.case_insensitive })}
          title={rule.case_insensitive ? "Case-insensitive (click to match case)" : "Case-sensitive"}
          className={[
            "px-2 py-1.5 text-[9px] font-mono border-l border-[rgba(255,255,255,0.06)] transition-colors",
            rule.case_insensitive
              ? "bg-transparent text-[rgba(255,255,255,0.3)] hover:text-[rgba(255,255,255,0.6)]"
              : "bg-[rgba(251,191,36,0.12)] text-[#fbbf24]",
          ].join(" ")}
        >
          Aa
        </button>
      </div>
      <button
        onClick={onDelete}
        title="Delete rule"
        className="w-5 h-5 rounded flex items-center justify-center text-[12px] leading-none text-[rgba(255,255,255,0.25)] opacity-0 group-hover:opacity-100 hover:text-[#ef4444] transition-all shrink-0"
      >
        ×
      </button>
    </div>
  );
}

export default function VocabTab({
  config,
  onSave,
}: {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
}) {
  const books = config.vocab_books ?? [];
  const globalIds = config.global_vocab_books ?? [];
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const saveBooks = (next: VocabBook[]) => onSave({ vocab_books: next });

  const updateBook = (id: string, patch: Partial<VocabBook>) => {
    saveBooks(books.map((b) => (b.id === id ? { ...b, ...patch } : b)));
  };

  const removeBook = (id: string) => {
    saveBooks(books.filter((b) => b.id !== id));
    onSave({ global_vocab_books: globalIds.filter((g) => g !== id) });
  };

  const toggleGlobal = (id: string) => {
    const next = globalIds.includes(id)
      ? globalIds.filter((g) => g !== id)
      : [...globalIds, id];
    onSave({ global_vocab_books: next });
  };

  const addBook = () => {
    const book = newBook();
    saveBooks([...books, book]);
    setExpandedId(book.id);
  };

  return (
    <div className="flex flex-col gap-4">
      <div>
        <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">Vocabulary books</div>
        <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
          Terms bias speech recognition and guide LLM output; rules are deterministic
          find → replace corrections applied to every transcript. Mark a book Global to
          apply it everywhere — or mount it on specific modes from each mode's card in
          the Dictation tab.
        </div>
      </div>

      <div className="flex flex-col gap-2">
        {books.map((book) => {
          const expanded = expandedId === book.id;
          const isGlobal = globalIds.includes(book.id);
          return (
            <div
              key={book.id}
              className={[
                "rounded-xl border transition-colors",
                expanded
                  ? "border-[rgba(251,191,36,0.15)] bg-[rgba(255,255,255,0.025)]"
                  : "border-[rgba(255,255,255,0.06)] bg-[rgba(255,255,255,0.02)] hover:border-[rgba(255,255,255,0.1)]",
              ].join(" ")}
            >
              {/* Book header row */}
              <div
                className="flex items-center gap-2.5 px-3 py-2.5 cursor-pointer select-none"
                onClick={() => setExpandedId(expanded ? null : book.id)}
              >
                <span
                  className={[
                    "w-1.5 h-1.5 rounded-full shrink-0",
                    book.enabled ? "bg-[#4ade80]" : "bg-[rgba(255,255,255,0.15)]",
                  ].join(" ")}
                />
                <span className="text-[11.5px] font-medium text-[#fafaf9] truncate">
                  {book.name || "(unnamed)"}
                </span>
                {isGlobal && (
                  <span className="px-1.5 py-0.5 rounded-full text-[8px] font-semibold uppercase tracking-wide bg-[rgba(245,158,11,0.12)] text-[#fbbf24] shrink-0">
                    Global
                  </span>
                )}
                <span className="flex-1" />
                <span className="text-[8.5px] px-1.5 py-0.5 rounded-full bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.35)] shrink-0">
                  {book.terms.length} terms
                </span>
                <span className="text-[8.5px] px-1.5 py-0.5 rounded-full bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.35)] shrink-0">
                  {book.rules.length} rules
                </span>
                <span
                  className={[
                    "text-[rgba(255,255,255,0.25)] text-[10px] transition-transform",
                    expanded ? "rotate-90" : "",
                  ].join(" ")}
                >
                  ▸
                </span>
              </div>

              {expanded && (
                <div className="px-3 pb-3 flex flex-col gap-3.5 border-t border-[rgba(255,255,255,0.04)] pt-3">
                  {/* Name + toggles */}
                  <div className="flex items-center gap-2">
                    <input
                      key={`name-${book.id}`}
                      type="text"
                      defaultValue={book.name}
                      onBlur={(e) => {
                        if (e.target.value !== book.name) updateBook(book.id, { name: e.target.value });
                      }}
                      placeholder="Book name"
                      className={`${input} flex-1`}
                    />
                    <button
                      onClick={() => toggleGlobal(book.id)}
                      className={[
                        "px-2.5 py-1.5 rounded-lg text-[10px] transition-all",
                        isGlobal
                          ? "bg-[rgba(245,158,11,0.12)] border border-[rgba(245,158,11,0.25)] text-[#fbbf24]"
                          : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.45)]",
                      ].join(" ")}
                    >
                      Global
                    </button>
                    <button
                      onClick={() => updateBook(book.id, { enabled: !book.enabled })}
                      className={[
                        "px-2.5 py-1.5 rounded-lg text-[10px] transition-all",
                        book.enabled
                          ? "bg-[rgba(74,222,128,0.1)] border border-[rgba(74,222,128,0.2)] text-[#4ade80]"
                          : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.3)]",
                      ].join(" ")}
                    >
                      {book.enabled ? "Enabled" : "Disabled"}
                    </button>
                    <button
                      onClick={() => removeBook(book.id)}
                      title="Delete book"
                      className="px-2 py-1.5 rounded-lg text-[13px] leading-none text-[rgba(255,255,255,0.3)] hover:text-[#ef4444] transition-colors"
                    >
                      ×
                    </button>
                  </div>

                  {/* Terms */}
                  <div className="flex flex-col gap-1.5">
                    <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
                      Terms
                      <span className="ml-1 text-[rgba(255,255,255,0.15)]">
                        — correct spellings the recognizer should prefer
                      </span>
                    </label>
                    <TermChips
                      terms={book.terms}
                      onChange={(terms) => updateBook(book.id, { terms })}
                    />
                  </div>

                  {/* Rules */}
                  <div className="flex flex-col gap-1">
                    <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
                      Correction rules
                      <span className="ml-1 text-[rgba(255,255,255,0.15)]">
                        — deterministic fixes, e.g. 衣袖 → issue
                      </span>
                    </label>
                    {book.rules.length === 0 && (
                      <div className="text-[9.5px] text-[rgba(255,255,255,0.18)] px-0.5 pb-0.5">
                        When the recognizer keeps mishearing a word, pin the fix here.
                      </div>
                    )}
                    {book.rules.map((rule, i) => (
                      <RuleRow
                        key={i}
                        rule={rule}
                        bookId={book.id}
                        index={i}
                        onPatch={(patch) =>
                          updateBook(book.id, {
                            rules: book.rules.map((r, idx) => (idx === i ? { ...r, ...patch } : r)),
                          })
                        }
                        onDelete={() =>
                          updateBook(book.id, { rules: book.rules.filter((_, idx) => idx !== i) })
                        }
                      />
                    ))}
                    <button
                      onClick={() => updateBook(book.id, { rules: [...book.rules, { ...EMPTY_RULE }] })}
                      className="w-full py-1.5 rounded-lg border border-dashed border-[rgba(255,255,255,0.1)] text-[10px] text-[rgba(255,255,255,0.3)] hover:text-[#fbbf24] hover:border-[rgba(251,191,36,0.3)] transition-colors"
                    >
                      + Add rule
                    </button>
                  </div>
                </div>
              )}
            </div>
          );
        })}

        {books.length === 0 && (
          <div className="flex flex-col items-center gap-2 py-8 text-center rounded-xl border border-dashed border-[rgba(255,255,255,0.08)]">
            <span className="text-[18px]">📖</span>
            <div className="text-[11px] text-[rgba(255,255,255,0.4)]">No vocabulary books yet</div>
            <div className="text-[10px] text-[rgba(255,255,255,0.22)] max-w-[300px]">
              Add one for your domain terms — names, jargon, product words — and dictation
              will start getting them right.
            </div>
          </div>
        )}

        <button
          onClick={addBook}
          className="self-start px-3 py-2 rounded-lg text-[11px] bg-[rgba(245,158,11,0.1)] border border-[rgba(245,158,11,0.25)] text-[#fbbf24] hover:bg-[rgba(245,158,11,0.15)] transition-colors"
        >
          + Add vocabulary book
        </button>
      </div>
    </div>
  );
}
