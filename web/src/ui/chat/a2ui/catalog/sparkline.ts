import { svg } from "lit";

export function renderSparkline(
  numbers: number[],
  width = 80,
  height = 24,
  color = "var(--primary, #4f8cff)",
) {
  if (numbers.length < 2) return svg``;

  const min = Math.min(...numbers);
  const max = Math.max(...numbers);
  const range = max - min || 1;

  const step = width / (numbers.length - 1);
  const points = numbers
    .map((v, i) => `${(i * step).toFixed(1)},${(height - ((v - min) / range) * (height - 4) - 2).toFixed(1)}`)
    .join(" ");

  const gradientId = `sparkline-fill-${Math.random().toString(36).slice(2, 8)}`;

  return svg`
    <svg width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"
         xmlns="http://www.w3.org/2000/svg" style="display:block">
      <defs>
        <linearGradient id="${gradientId}" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stop-color="${color}" stop-opacity="0.3"/>
          <stop offset="100%" stop-color="${color}" stop-opacity="0.05"/>
        </linearGradient>
      </defs>
      <polygon
        points="${points} ${width},${height} 0,${height}"
        fill="url(#${gradientId})"
      />
      <polyline
        points="${points}"
        fill="none"
        stroke="${color}"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  `;
}
