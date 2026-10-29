import { useEffect, useRef } from "react";

/**
 * Canvas starfield backdrop.
 *
 * Rendered as its own layer inside `AppBackground`; the layer is
 * `pointer-events-none` and sits at a negative z-index so it never intercepts
 * clicks. Motion is skipped when the OS asks for reduced motion: a single
 * static frame is painted instead, which keeps the texture without animating.
 */

export type StarfieldTone = "light" | "dark";

interface StarfieldProps {
  /**
   * Star tint. Pale stars read on the pitch-black palette; ink-toned stars are
   * needed on the light palette or they disappear into the white base layer.
   */
  tone?: StarfieldTone;
  /** Lets the caller unmount the animation without tearing down the layer. */
  enabled?: boolean;
}

interface Star {
  x: number;
  y: number;
  radius: number;
  velocityX: number;
  velocityY: number;
  baseOpacity: number;
  twinkleSpeed: number;
  twinklePhase: number;
}

const MIN_STARS = 40;
const MAX_STARS = 140;
const AREA_PER_STAR = 9000;
const DARK_STAR_RGB = "220, 220, 235";
const LIGHT_STAR_RGB = "51, 51, 51";

export function Starfield({ tone = "dark", enabled = true }: StarfieldProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const element = canvasRef.current;
    if (!enabled || !element) return undefined;

    const context = element.getContext("2d");
    if (!context) return undefined;

    const prefersReducedMotion =
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const starColor = tone === "dark" ? DARK_STAR_RGB : LIGHT_STAR_RGB;

    let width = 0;
    let height = 0;
    let stars: Star[] = [];
    let frameId = 0;

    const createStar = (): Star => ({
      x: Math.random() * width,
      y: Math.random() * height,
      radius: Math.random() * 1.5 + 0.3,
      velocityX: (Math.random() - 0.5) * 0.15,
      velocityY: (Math.random() - 0.5) * 0.15,
      baseOpacity: Math.random() * 0.7 + 0.3,
      twinkleSpeed: Math.random() * 0.008 + 0.003,
      twinklePhase: Math.random() * Math.PI * 2,
    });

    const reseedStars = () => {
      const density = Math.floor((width * height) / AREA_PER_STAR);
      const count = Math.min(MAX_STARS, Math.max(MIN_STARS, density));
      stars = Array.from({ length: count }, createStar);
    };

    const paint = (now: number, advance: boolean) => {
      context.clearRect(0, 0, width, height);

      for (const star of stars) {
        if (advance) {
          star.x += star.velocityX;
          star.y += star.velocityY;
          // Wrap just outside the viewport so drift never leaves dead edges.
          if (star.x < -5) star.x = width + 5;
          if (star.x > width + 5) star.x = -5;
          if (star.y < -5) star.y = height + 5;
          if (star.y > height + 5) star.y = -5;
        }

        const twinkle =
          0.5 + 0.5 * Math.sin(now * star.twinkleSpeed + star.twinklePhase);
        const alpha = star.baseOpacity * twinkle;

        context.beginPath();
        context.arc(star.x, star.y, star.radius, 0, Math.PI * 2);
        context.fillStyle = `rgba(${starColor}, ${alpha})`;
        context.fill();
      }
    };

    const resize = () => {
      const pixelRatio = Math.min(window.devicePixelRatio || 1, 2);
      // clientWidth/clientHeight are CSS pixels; the backing store is scaled so
      // stars stay crisp on HiDPI displays without changing the layout size.
      width = element.clientWidth || window.innerWidth;
      height = element.clientHeight || window.innerHeight;
      element.width = Math.max(1, Math.round(width * pixelRatio));
      element.height = Math.max(1, Math.round(height * pixelRatio));
      context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
      reseedStars();
      if (prefersReducedMotion) paint(0, false);
    };

    const loop = () => {
      paint(Date.now(), true);
      frameId = requestAnimationFrame(loop);
    };

    resize();

    if (!prefersReducedMotion) {
      frameId = requestAnimationFrame(loop);
    }

    window.addEventListener("resize", resize);

    return () => {
      cancelAnimationFrame(frameId);
      window.removeEventListener("resize", resize);
      // Release the backing store; the component may remount on theme change.
      element.width = 0;
      element.height = 0;
    };
  }, [enabled, tone]);

  return (
    <canvas
      ref={canvasRef}
      aria-hidden="true"
      className="pointer-events-none absolute inset-0 h-full w-full opacity-80"
    />
  );
}
