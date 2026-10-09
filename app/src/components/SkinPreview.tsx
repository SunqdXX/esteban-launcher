import styles from "./SkinPreview.module.css";

interface SkinPreviewProps {
  rows: string[];
  head?: boolean;
  label: string;
}

const SILHOUETTE: [number, number, number, number][] = [
  [4, 0, 8, 8],
  [4, 8, 8, 12],
  [0, 8, 4, 12],
  [12, 8, 4, 12],
  [4, 20, 4, 12],
  [8, 20, 4, 12],
];

export function Silhouette({ head, label }: { head?: boolean; label: string }) {
  return (
    <svg className={styles.skin} viewBox={head ? "4 0 8 8" : "0 0 16 32"} shapeRendering="crispEdges" role="img" aria-label={label}>
      {SILHOUETTE.map(([x, y, w, h]) => (
        <rect key={`${String(x)}-${String(y)}`} x={x} y={y} width={w} height={h} className={styles.blank} />
      ))}
    </svg>
  );
}

export default function SkinPreview({ rows, head, label }: SkinPreviewProps) {
  const cells: { x: number; y: number; fill: string; opacity: number }[] = [];
  rows.forEach((row, y) => {
    if (head && y >= 8) return;
    for (let x = 0; x < 16; x++) {
      if (head && (x < 4 || x >= 12)) continue;
      const hex = row.slice(x * 8, x * 8 + 8);
      const alpha = parseInt(hex.slice(6, 8), 16);
      if (!alpha) continue;
      cells.push({ x, y, fill: `#${hex.slice(0, 6)}`, opacity: alpha / 255 });
    }
  });
  return (
    <svg className={styles.skin} viewBox={head ? "4 0 8 8" : "0 0 16 32"} shapeRendering="crispEdges" role="img" aria-label={label}>
      {cells.map((c) => (
        <rect key={`${String(c.x)}-${String(c.y)}`} x={c.x} y={c.y} width="1" height="1" fill={c.fill} fillOpacity={c.opacity} />
      ))}
    </svg>
  );
}
