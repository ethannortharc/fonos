// Voice view — TTS synthesis + voice library (merged).
// Full implementation in WP-08.

import { useState, useEffect, useCallback } from "react";
import {
  listVoices,
  generateAndPlay,
  cloneVoice,
  deleteVoice,
  previewVoice,
  pickAudioFile,
  recordVoiceSample,
  stopPlayback,
} from "../lib/api";
import type { VoiceEntry, TtsResult } from "../types";

export default function Voice() {
  const [voices, setVoices] = useState<VoiceEntry[]>([]);
  const [selectedVoice, setSelectedVoice] = useState<string>("default");
  const [ttsText, setTtsText] = useState<string>("");
  const [speed, setSpeed] = useState<number>(1.0);
  const [isPlaying, setIsPlaying] = useState<boolean>(false);
  const [ttsResult, setTtsResult] = useState<TtsResult | null>(null);
  const [error, setError] = useState<string>("");
  const [cloneName, setCloneName] = useState<string>("");
  const [showClone, setShowClone] = useState<boolean>(false);
  const [isRecordingVoice, setIsRecordingVoice] = useState(false);
  const [recordSecs, setRecordSecs] = useState(0);
  const [recordDuration, setRecordDuration] = useState(5);
  const [cloneSuccess, setCloneSuccess] = useState(false);

  const loadVoices = useCallback(() => {
    listVoices()
      .then((res) => {
        setVoices(res.voices);
        // Default to "default" voice if not set
        if (!res.voices.find((v) => v.voice_id === selectedVoice)) {
          setSelectedVoice("default");
        }
      })
      .catch((e: unknown) => console.error("listVoices:", e));
  }, [selectedVoice]);

  useEffect(() => {
    loadVoices();
  }, [loadVoices]);

  const handleGenerate = useCallback(async () => {
    if (!ttsText.trim()) return;
    setError("");
    setIsPlaying(true);
    try {
      const result = await generateAndPlay(ttsText, selectedVoice, speed);
      setTtsResult(result);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsPlaying(false);
    }
  }, [ttsText, selectedVoice, speed]);

  const handleStop = useCallback(async () => {
    await stopPlayback().catch(() => {});
    setIsPlaying(false);
  }, []);

  const handleCloneFromFile = useCallback(async () => {
    if (!cloneName.trim()) {
      setError("Enter a name for the voice");
      return;
    }
    setError("");
    try {
      const path = await pickAudioFile();
      if (!path) return;
      await cloneVoice(cloneName, path);
      loadVoices();
      setCloneSuccess(true);
      setTimeout(() => { setShowClone(false); setCloneSuccess(false); setCloneName(""); }, 1200);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [cloneName, loadVoices]);

  const handleCloneFromRecord = useCallback(async () => {
    if (!cloneName.trim()) {
      setError("Enter a name for the voice");
      return;
    }
    setError("");
    setIsRecordingVoice(true);
    setRecordSecs(0);
    const start = Date.now();
    const timer = setInterval(() => setRecordSecs((Date.now() - start) / 1000), 100);
    try {
      const path = await recordVoiceSample(recordDuration);
      clearInterval(timer);
      setIsRecordingVoice(false);
      await cloneVoice(cloneName, path);
      loadVoices();
      setCloneSuccess(true);
      setTimeout(() => { setShowClone(false); setCloneSuccess(false); setCloneName(""); }, 1200);
    } catch (e: unknown) {
      clearInterval(timer);
      setIsRecordingVoice(false);
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [cloneName, recordDuration, loadVoices]);

  const handleDeleteVoice = useCallback(
    async (voiceId: string) => {
      try {
        await deleteVoice(voiceId);
        loadVoices();
        if (selectedVoice === voiceId) setSelectedVoice("default");
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [loadVoices, selectedVoice]
  );

  const handlePreview = useCallback(
    async (voiceId: string) => {
      try {
        await previewVoice(voiceId, "");
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    []
  );

  // Speed slider math
  const sliderMin = 0.5;
  const sliderMax = 2.0;
  const sliderPercent = ((speed - sliderMin) / (sliderMax - sliderMin)) * 100;

  return (
    <div className="flex flex-col h-full p-5 gap-3 bg-[#1a1917] overflow-auto">
      {/* Voice selector strip */}
      <div className="flex flex-col gap-2">
        <span className="text-[rgba(255,255,255,0.3)] text-[10px] uppercase tracking-wider font-medium">
          Voice
        </span>
        <div className="flex gap-1.5 flex-wrap">
          {voices.map((v) => (
            <div key={v.voice_id} className="flex items-center gap-1">
              <button
                onClick={() => setSelectedVoice(v.voice_id)}
                className={[
                  "px-3 py-1.5 rounded-lg text-xs font-medium transition-colors",
                  selectedVoice === v.voice_id
                    ? "bg-[rgba(245,158,11,0.12)] text-[#fbbf24]"
                    : "bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.35)] hover:bg-[rgba(255,255,255,0.07)]",
                ].join(" ")}
              >
                {v.name}
              </button>
              {v.voice_id !== "default" && (
                <>
                  <button
                    onClick={() => handlePreview(v.voice_id)}
                    className="text-[rgba(255,255,255,0.25)] hover:text-[rgba(255,255,255,0.5)] text-xs px-1 transition-colors"
                    title="Preview"
                  >
                    ▶
                  </button>
                  <button
                    onClick={() => handleDeleteVoice(v.voice_id)}
                    className="text-[rgba(255,255,255,0.25)] hover:text-[#ef4444] text-xs px-1 transition-colors"
                    title="Delete"
                  >
                    ✕
                  </button>
                </>
              )}
            </div>
          ))}
          {/* + Clone chip */}
          <button
            onClick={() => setShowClone((prev) => !prev)}
            className="rounded-lg border border-dashed border-[rgba(245,158,11,0.2)] text-[rgba(251,191,36,0.5)] px-3 py-1.5 text-xs transition-colors hover:border-[rgba(245,158,11,0.4)] hover:text-[rgba(251,191,36,0.7)]"
          >
            + Clone
          </button>
        </div>
      </div>

      {/* Clone voice modal */}
      {showClone && (
        <div className="fixed inset-0 z-50 flex items-center justify-center" onClick={() => !isRecordingVoice && !cloneSuccess && setShowClone(false)}>
          <div className="absolute inset-0 bg-black/40 backdrop-blur-[2px]" />
          <div
            className="relative w-72 bg-[#242220] border border-[rgba(255,255,255,0.08)] rounded-2xl p-5 shadow-2xl flex flex-col gap-3"
            onClick={(e) => e.stopPropagation()}
          >
            {cloneSuccess ? (
              <div className="flex flex-col items-center gap-2 py-6">
                <div className="w-10 h-10 rounded-full bg-[rgba(134,239,172,0.1)] flex items-center justify-center text-[#86efac] text-lg font-bold">
                  &#10003;
                </div>
                <span className="text-[13px] text-[#fafaf9]">Voice cloned!</span>
              </div>
            ) : (
              <>
                <div className="text-[13px] font-medium text-[#fafaf9]">Clone Voice</div>
                <input
                  type="text"
                  value={cloneName}
                  onChange={(e) => setCloneName(e.target.value)}
                  placeholder="Voice name..."
                  disabled={isRecordingVoice}
                  className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[13px] placeholder:text-[rgba(255,255,255,0.2)] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                />
                {isRecordingVoice ? (
                  <div className="flex items-center gap-3 px-1 py-1">
                    <div className="w-2 h-2 rounded-full bg-[#ef4444] animate-pulse flex-shrink-0" />
                    <span className="text-[12px] text-[rgba(255,255,255,0.4)] font-mono flex-shrink-0">
                      {recordSecs.toFixed(1)}s / {recordDuration}s
                    </span>
                    <div className="flex-1 h-1 bg-[rgba(255,255,255,0.06)] rounded-full overflow-hidden">
                      <div
                        className="h-full bg-[#ef4444] rounded-full transition-all duration-100"
                        style={{ width: `${Math.min(100, (recordSecs / recordDuration) * 100)}%` }}
                      />
                    </div>
                  </div>
                ) : (
                  <>
                    <div className="flex items-center gap-2">
                      <span className="text-[10px] text-[rgba(255,255,255,0.25)]">Duration</span>
                      <div className="flex gap-1">
                        {[3, 5, 8, 10].map((d) => (
                          <button
                            key={d}
                            onClick={() => setRecordDuration(d)}
                            className={[
                              "px-2 py-0.5 rounded text-[10px] transition-colors",
                              d === recordDuration
                                ? "bg-[rgba(245,158,11,0.12)] text-[#fbbf24]"
                                : "bg-[rgba(255,255,255,0.03)] text-[rgba(255,255,255,0.25)] hover:text-[rgba(255,255,255,0.4)]",
                            ].join(" ")}
                          >
                            {d}s
                          </button>
                        ))}
                      </div>
                    </div>
                    <div className="flex gap-2">
                      <button
                        onClick={handleCloneFromRecord}
                        className="flex-1 flex items-center justify-center gap-1.5 px-3 py-2.5 bg-[rgba(255,255,255,0.04)] hover:bg-[rgba(255,255,255,0.08)] text-[rgba(255,255,255,0.4)] text-[13px] rounded-lg transition-colors"
                      >
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
                          <rect x="9" y="1" width="6" height="12" rx="3" /><path d="M5 10a7 7 0 0 0 14 0" /><line x1="12" y1="17" x2="12" y2="21" />
                        </svg>
                        Record
                      </button>
                      <button
                        onClick={handleCloneFromFile}
                        className="flex-1 flex items-center justify-center gap-1.5 px-3 py-2.5 bg-[rgba(255,255,255,0.04)] hover:bg-[rgba(255,255,255,0.08)] text-[rgba(255,255,255,0.4)] text-[13px] rounded-lg transition-colors"
                      >
                        Pick File
                      </button>
                    </div>
                  </>
                )}
                {!isRecordingVoice && (
                  <button
                    onClick={() => { setShowClone(false); setCloneName(""); }}
                    className="text-[11px] text-[rgba(255,255,255,0.2)] hover:text-[rgba(255,255,255,0.4)] transition-colors self-center pt-1"
                  >
                    Cancel
                  </button>
                )}
              </>
            )}
          </div>
        </div>
      )}

      {/* TTS synthesis — text input */}
      <div className="flex flex-col gap-2 flex-1">
        <span className="text-[rgba(255,255,255,0.3)] text-[10px] uppercase tracking-wider font-medium">
          Synthesize Speech
        </span>
        <textarea
          value={ttsText}
          onChange={(e) => setTtsText(e.target.value)}
          placeholder="Enter text to synthesize..."
          className="flex-1 min-h-24 bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2.5 text-[#fafaf9] text-[13px] leading-relaxed placeholder:text-[rgba(255,255,255,0.2)] focus:outline-none focus:border-[rgba(245,158,11,0.3)] resize-none"
        />
      </div>

      {/* Speed slider — custom styled */}
      <div className="flex items-center gap-2.5">
        <span className="text-[10px] text-[rgba(255,255,255,0.3)] w-10">Speed</span>
        <div className="flex-1 h-1 bg-[rgba(255,255,255,0.06)] rounded-full relative cursor-pointer">
          {/* Filled portion */}
          <div
            className="absolute left-0 top-0 h-full rounded-full bg-gradient-to-r from-[rgba(251,191,36,0.3)] to-[#fbbf24]"
            style={{ width: `${sliderPercent}%` }}
          />
          {/* Knob */}
          <div
            className="absolute top-1/2 -translate-y-1/2 w-3 h-3 rounded-full bg-[#fbbf24] shadow-[0_2px_6px_rgba(251,191,36,0.3)]"
            style={{ left: `${sliderPercent}%`, marginLeft: "-6px" }}
          />
          {/* Hidden native range for interaction */}
          <input
            type="range"
            min={sliderMin}
            max={sliderMax}
            step={0.05}
            value={speed}
            onChange={(e) => setSpeed(parseFloat(e.target.value))}
            className="absolute inset-0 w-full h-full opacity-0 cursor-pointer"
          />
        </div>
        <span className="text-[10px] text-[rgba(255,255,255,0.4)] w-8 text-right font-mono">
          {speed.toFixed(2)}x
        </span>
      </div>

      {/* Generate + playback bar */}
      <button
        onClick={isPlaying ? handleStop : handleGenerate}
        disabled={!ttsText.trim() && !isPlaying}
        className={[
          "w-full py-2.5 rounded-lg text-[13px] font-semibold transition-colors",
          isPlaying
            ? "animate-pulse bg-[rgba(245,158,11,0.1)] text-[#fbbf24]"
            : "bg-gradient-to-br from-[#f59e0b] to-[#d97706] text-[#1a1917]",
          !ttsText.trim() && !isPlaying
            ? "opacity-40 cursor-not-allowed"
            : "",
        ].join(" ")}
      >
        {isPlaying ? "Generating..." : "Generate & Play"}
      </button>

      {/* Error */}
      {error && (
        <div className="rounded-lg bg-[rgba(239,68,68,0.08)] border border-[rgba(239,68,68,0.15)] p-3">
          <p className="text-[#ef4444] text-xs">{error}</p>
        </div>
      )}

      {/* Result info */}
      {ttsResult && (
        <div className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)] rounded-[10px] p-3">
          <p className="text-[rgba(255,255,255,0.35)] text-xs">
            {ttsResult.duration_secs.toFixed(1)}s · {ttsResult.latency_ms}ms ·{" "}
            {(ttsResult.size_bytes / 1024).toFixed(0)}KB
          </p>
        </div>
      )}
    </div>
  );
}
