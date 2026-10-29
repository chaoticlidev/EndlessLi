import { useAppearance } from "../stores/appearance";
import type { ResolvedTheme } from "../stores/appearance";
import { Starfield } from "./Starfield";

/**
 * The layered window backdrop.
 *
 * Layer order, bottom to top:
 *   1. `bg-background` on this wrapper - the flat base color.
 *   2. Starfield canvas, custom image, or nothing.
 *   3. A black scrim driven by `backgroundDim` so UI text stays readable over
 *      a busy photo.
 *   4. The actual UI, which is rendered as a sibling by `App`.
 *
 * The wrapper paints at `-z-10` inside the app shell's `isolate` stacking
 * context, so it sits above the shell background but below every panel.
 */
export function AppBackground({ tone }: { tone: ResolvedTheme }) {
  const mode = useAppearance((state) => state.background);
  const image = useAppearance((state) => state.backgroundImage);
  const dim = useAppearance((state) => state.backgroundDim);

  const showsImage = mode === "image" && image !== null;

  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute inset-0 -z-10 overflow-hidden bg-background"
    >
      {mode === "starfield" && <Starfield tone={tone} />}

      {showsImage && (
        <div
          className="absolute inset-0 bg-cover bg-center bg-no-repeat"
          style={{ backgroundImage: `url("${image}")` }}
        />
      )}

      {/* The scrim exists for photographic backgrounds only; applying it to the
          starfield would gray out the base color and hide the stars. */}
      {showsImage && dim > 0 && (
        <div
          className="absolute inset-0 bg-black"
          style={{ opacity: dim / 100 }}
        />
      )}
    </div>
  );
}
