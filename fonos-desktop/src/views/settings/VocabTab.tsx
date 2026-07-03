// Vocabulary settings — user-defined vocab books (terms + correction rules).
// Books are referenced by id from global_vocab_books and per-mode vocab_books.

import { useState } from "react";
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
  // Terms are edited as raw textarea text (one per line) and parsed on blur,
  // so typing never fights a re-serialize round-trip.
  const [termsDraft, setTermsDraft] = useState<{ id: string; text: string } | null>(null);

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

  const termsText = (book: VocabBook) =>
    termsDraft && termsDraft.id === book.id ? termsDraft.text : book.terms.join("\n");

  const commitTerms = (book: VocabBook) => {
    if (!termsDraft || termsDraft.id !== book.id) return;
    const terms = termsDraft.text
      .split("\n")
      .map((t) => t.trim())
      .filter((t) => t !== "");
    updateBook(book.id, { terms });
    setTermsDraft(null);
  };

  const updateRule = (book: VocabBook, i: number, patch: Partial<VocabRule>) => {
    updateBook(book.id, {
      rules: book.rules.map((r, idx) => (idx === i ? { ...r, ...patch } : r)),
    });
  };

  const input =
    "bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2.5 py-1.5 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]";

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
              className="rounded-xl border border-[rgba(255,255,255,0.06)] bg-[rgba(255,255,255,0.02)]"
            >
              {/* Book header row */}
              <div
                className="flex items-center gap-2 px-3 py-2.5 cursor-pointer"
                onClick={() => setExpandedId(expanded ? null : book.id)}
              >
                <span className="text-[11px] font-medium text-[#fafaf9] flex-1 truncate">
                  {book.name || "(unnamed)"}
                </span>
                <span className="text-[9px] text-[rgba(255,255,255,0.25)]">
                  {book.terms.length} terms · {book.rules.length} rules
                </span>
                {isGlobal && (
                  <span className="px-1.5 py-0.5 rounded text-[8px] font-semibold uppercase tracking-wide bg-[rgba(245,158,11,0.12)] text-[#fbbf24]">
                    Global
                  </span>
                )}
                {!book.enabled && (
                  <span className="px-1.5 py-0.5 rounded text-[8px] uppercase bg-[rgba(255,255,255,0.05)] text-[rgba(255,255,255,0.3)]">
                    Off
                  </span>
                )}
                <span className="text-[rgba(255,255,255,0.25)] text-[10px]">{expanded ? "▾" : "▸"}</span>
              </div>

              {expanded && (
                <div className="px-3 pb-3 flex flex-col gap-3 border-t border-[rgba(255,255,255,0.04)] pt-3">
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
                  <div className="flex flex-col gap-1">
                    <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
                      Terms
                      <span className="ml-1 text-[rgba(255,255,255,0.15)]">
                        (one per line — correct spellings, e.g. Kubernetes)
                      </span>
                    </label>
                    <textarea
                      value={termsText(book)}
                      onChange={(e) => setTermsDraft({ id: book.id, text: e.target.value })}
                      onBlur={() => commitTerms(book)}
                      rows={4}
                      spellCheck={false}
                      className={`${input} resize-y font-mono leading-relaxed`}
                    />
                  </div>

                  {/* Rules */}
                  <div className="flex flex-col gap-1.5">
                    <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
                      Correction rules
                      <span className="ml-1 text-[rgba(255,255,255,0.15)]">
                        (find → replace, e.g. "my sequel" → "MySQL")
                      </span>
                    </label>
                    {book.rules.map((rule, i) => (
                      <div key={i} className="flex items-center gap-1.5">
                        <input
                          key={`from-${book.id}-${i}`}
                          type="text"
                          defaultValue={rule.from}
                          onBlur={(e) => {
                            if (e.target.value !== rule.from) updateRule(book, i, { from: e.target.value });
                          }}
                          placeholder="find"
                          spellCheck={false}
                          className={`${input} flex-1 min-w-0`}
                        />
                        <span className="text-[rgba(255,255,255,0.25)] text-[10px]">→</span>
                        <input
                          key={`to-${book.id}-${i}`}
                          type="text"
                          defaultValue={rule.to}
                          onBlur={(e) => {
                            if (e.target.value !== rule.to) updateRule(book, i, { to: e.target.value });
                          }}
                          placeholder="replace"
                          spellCheck={false}
                          className={`${input} flex-1 min-w-0`}
                        />
                        <button
                          onClick={() =>
                            updateRule(book, i, { kind: rule.kind === "regex" ? "literal" : "regex" })
                          }
                          title={rule.kind === "regex" ? "Regex pattern" : "Literal text"}
                          className={[
                            "px-2 py-1.5 rounded-lg text-[9px] font-mono transition-all flex-shrink-0",
                            rule.kind === "regex"
                              ? "bg-[rgba(192,132,252,0.12)] border border-[rgba(192,132,252,0.25)] text-[#c084fc]"
                              : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)]",
                          ].join(" ")}
                        >
                          .*
                        </button>
                        <button
                          onClick={() =>
                            updateRule(book, i, { case_insensitive: !rule.case_insensitive })
                          }
                          title={rule.case_insensitive ? "Case-insensitive" : "Case-sensitive"}
                          className={[
                            "px-2 py-1.5 rounded-lg text-[9px] font-mono transition-all flex-shrink-0",
                            rule.case_insensitive
                              ? "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)]"
                              : "bg-[rgba(251,191,36,0.1)] border border-[rgba(245,158,11,0.25)] text-[#fbbf24]",
                          ].join(" ")}
                        >
                          Aa
                        </button>
                        <button
                          onClick={() =>
                            updateBook(book.id, { rules: book.rules.filter((_, idx) => idx !== i) })
                          }
                          className="px-1.5 py-1.5 rounded-lg text-[13px] leading-none text-[rgba(255,255,255,0.3)] hover:text-[#fbbf24] transition-colors flex-shrink-0"
                        >
                          ×
                        </button>
                      </div>
                    ))}
                    <button
                      onClick={() => updateBook(book.id, { rules: [...book.rules, { ...EMPTY_RULE }] })}
                      className="text-[10px] text-[rgba(251,191,36,0.5)] hover:text-[#fbbf24] transition-colors self-start"
                    >
                      Add rule
                    </button>
                  </div>
                </div>
              )}
            </div>
          );
        })}

        {books.length === 0 && (
          <div className="text-[11px] text-[rgba(255,255,255,0.25)] py-4 text-center">
            No vocabulary books yet. Add one for your domain terms — names, jargon,
            product words — and dictation will start getting them right.
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
