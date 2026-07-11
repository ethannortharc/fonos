// 新建配方弹窗：名称 + 来源二选一（来源决定配方形态），选项内嵌种子链预览。
import { useEffect, useState } from "react";
import { t, useT } from "../../lib/i18n";

export default function NewRecipeModal({
  open, onClose, onCreate,
}: {
  open: boolean;
  onClose: () => void;
  onCreate: (name: string, src: "mic" | "sel") => void;
}) {
  useT();
  const [name, setName] = useState("");
  const [src, setSrc] = useState<"mic" | "sel">("mic");

  // While open: reset the form to a clean slate, and let Escape close the
  // modal (document-level listener — the modal has no single focused root to
  // attach a key handler to). Cleaned up on close/unmount.
  useEffect(() => {
    if (!open) return;
    setName("");
    setSrc("mic");
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  if (!open) return null;
  const opt = (k: "mic" | "sel", title: string, desc: string, seed: string) => (
    <button
      onClick={() => setSrc(k)}
      className={[
        "rounded-[10px] border p-3 text-left transition-colors",
        src === k
          ? "border-[rgba(251,191,36,0.5)] bg-[rgba(251,191,36,0.06)]"
          : "border-[rgba(255,255,255,0.075)] bg-[rgba(255,255,255,0.025)] hover:border-[rgba(255,255,255,0.13)]",
      ].join(" ")}
    >
      <div className="text-[12px] font-semibold text-[rgba(251,191,36,0.95)]">{title}</div>
      <div className="mt-1 text-[10px] leading-[1.5] text-[rgba(255,255,255,0.43)]">{desc}</div>
      <div className="mt-2 text-[10px] text-[rgba(255,255,255,0.28)]">{seed}</div>
    </button>
  );
  return (
    <div className="fixed inset-0 z-30 flex items-center justify-center bg-[rgba(10,9,7,0.55)] backdrop-blur-[3px]"
         onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="w-[460px] rounded-[12px] border border-[rgba(255,255,255,0.075)] bg-[#1c1a17] p-5 shadow-2xl">
        <div className="text-[13px] font-semibold">{t("wb.newrecipe.title")}</div>
        <div className="mt-0.5 mb-3.5 text-[10.5px] text-[rgba(255,255,255,0.43)]">{t("wb.newrecipe.note")}</div>
        <div className="text-[10px] text-[rgba(255,255,255,0.43)] mb-1">{t("wf.field.name")}</div>
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t("wb.newrecipe.name-ph")}
          className="w-full rounded-[8px] border border-[rgba(255,255,255,0.075)] bg-[rgba(255,255,255,0.035)] px-2.5 py-[7px] text-[12px] outline-none focus:border-[rgba(242,184,75,0.5)]"
        />
        <div className="text-[10px] text-[rgba(255,255,255,0.43)] mt-3 mb-1">{t("wb.newrecipe.source")}</div>
        <div className="grid grid-cols-2 gap-2.5">
          {opt("mic", t("wb.newrecipe.mic"), t("wb.newrecipe.mic-desc"), t("wb.newrecipe.mic-seed"))}
          {opt("sel", t("wb.newrecipe.sel"), t("wb.newrecipe.sel-desc"), t("wb.newrecipe.sel-seed"))}
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button onClick={onClose}
            className="rounded-[8px] border border-[rgba(255,255,255,0.075)] px-3 py-[5px] text-[11px] text-[rgba(255,255,255,0.62)]">
            {t("common.cancel")}
          </button>
          <button onClick={() => onCreate(name.trim(), src)}
            className="rounded-[8px] px-3 py-[5px] text-[11px] font-semibold text-[#45300e]"
            style={{ background: "linear-gradient(148deg,#ffdd85 0%,#f5b043 46%,#d67e1c 78%)" }}>
            {t("wb.newrecipe.create")}
          </button>
        </div>
      </div>
    </div>
  );
}
