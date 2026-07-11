// WaveCanvas — the jumping-color-blocks recording waveform, extracted
// verbatim (renamed from JumpingBlocks) from the former Dictation.tsx so the
// Test Run bench (Task 11) can render the same live-mic feedback for mock
// audio input. Dictation.tsx itself has since been deleted (Task 12) — this
// component no longer depends on it.

import { useEffect, useRef } from "react";

export default function WaveCanvas({ active }: { active: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animRef = useRef<number>(0);
  const tRef = useRef(0);
  const fadeRef = useRef(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);
    const W = rect.width;
    const H = rect.height;
    const barCount = 36;
    const gap = 3;
    // Don't span full width — leave ~15% margin on each side
    const margin = W * 0.12;
    const usableW = W - margin * 2;
    const barW = (usableW - (barCount - 1) * gap) / barCount;
    const maxH = H * 0.34;
    const baseY = H * 0.72;

    const draw = () => {
      const target = active ? 1 : 0;
      fadeRef.current += (target - fadeRef.current) * 0.04;
      const fade = fadeRef.current;

      ctx.clearRect(0, 0, W, H);
      if (fade < 0.005) { animRef.current = requestAnimationFrame(draw); return; }

      tRef.current += 0.03;
      const t = tRef.current;

      for (let i = 0; i < barCount; i++) {
        const x = margin + i * (barW + gap);

        // Normalized position: -1 (left edge) to +1 (right edge)
        const nx = (i - (barCount - 1) / 2) / ((barCount - 1) / 2);
        const absNx = Math.abs(nx);

        // Sound radiating from mic center:
        // - Near center (mic): short bars (mic is there)
        // - Mid range: tallest bars (sound radiates outward)
        // - Edges: bars shrink and fade (sound dissipates)
        const micClear = 1 - Math.exp(-absNx * absNx * 8); // 0 at center, ~1 at mid
        const edgeDecay = Math.max(0, 1 - Math.pow(absNx, 2.5) * 1.2); // 1 at mid, 0 at edges
        const heightEnvelope = micClear * edgeDecay;

        // Organic height — layered waves
        const h1 = Math.sin(i * 0.4 + t * 1.2) * 0.5 + 0.5;
        const h2 = Math.sin(i * 0.7 + t * 0.8 + 2) * 0.3 + 0.5;
        const h3 = Math.sin(i * 0.2 + t * 1.6 - 1) * 0.2 + 0.5;
        const h = (h1 * 0.5 + h2 * 0.3 + h3 * 0.2) * maxH * heightEnvelope * fade;
        const barH = Math.max(1.5, h);

        // Opacity: bright near-center, smoothly fades to invisible at edges
        const opacityEnvelope = micClear * Math.max(0, 1 - Math.pow(absNx, 1.8) * 0.95);
        const opacity = (0.07 + opacityEnvelope * 0.34) * fade;
        ctx.fillStyle = `rgba(240, 173, 50, ${opacity})`;
        ctx.beginPath();
        ctx.roundRect(x, baseY - barH, barW, barH, 1.5);
        ctx.fill();
      }

      animRef.current = requestAnimationFrame(draw);
    };

    animRef.current = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(animRef.current);
  }, [active]);

  return <canvas ref={canvasRef} className="absolute inset-0 w-full h-full pointer-events-none" />;
}
