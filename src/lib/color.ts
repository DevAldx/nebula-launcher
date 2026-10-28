/** Utilidades pequeñas para el color de acento personalizado. */

export interface AccentColors {
  accent: string;
  hover: string;
  glow: string;
}

function parseHex(value: string): [number, number, number] | null {
  const match = /^#?([0-9a-fA-F]{6})$/.exec(value.trim());
  const hex = match?.[1];
  if (!hex) return null;
  const n = Number.parseInt(hex, 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

/** Acepta `#RRGGBB` o `RRGGBB` y devuelve siempre `#rrggbb`. */
export function normalizeHex(value: string): string | null {
  const rgb = parseHex(value);
  if (!rgb) return null;
  return `#${rgb.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}

function mix(hex: string, target: [number, number, number], amount: number): string {
  const rgb = parseHex(hex);
  if (!rgb) return hex;
  return `#${rgb
    .map((v, i) => Math.round(v + ((target[i] ?? v) - v) * amount))
    .map((v) => v.toString(16).padStart(2, "0"))
    .join("")}`;
}

export function accentColors(value: string | null): AccentColors | null {
  const accent = value ? normalizeHex(value) : null;
  const rgb = accent ? parseHex(accent) : null;
  if (!accent || !rgb) return null;
  return {
    accent,
    hover: mix(accent, [255, 255, 255], 0.24),
    glow: `rgba(${rgb[0]}, ${rgb[1]}, ${rgb[2]}, 0.38)`,
  };
}
