type Cell = readonly [number, number];

interface PixelIconProps {
  cells: readonly Cell[];
}

export default function PixelIcon({ cells }: PixelIconProps) {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" shapeRendering="crispEdges" aria-hidden="true">
      {cells.map(([x, y]) => (
        <rect key={`${String(x)}-${String(y)}`} x={x} y={y} width="1" height="1" fill="currentColor" />
      ))}
    </svg>
  );
}

const range = (from: number, to: number) => Array.from({ length: to - from + 1 }, (_, i) => from + i);

export const MINIMIZE: readonly Cell[] = range(1, 8).map((x) => [x, 8] as const);

export const MAXIMIZE: readonly Cell[] = [
  ...range(1, 8).map((x) => [x, 1] as const),
  ...range(1, 8).map((x) => [x, 8] as const),
  ...range(2, 7).map((y) => [1, y] as const),
  ...range(2, 7).map((y) => [8, y] as const),
];

export const CLOSE: readonly Cell[] = [
  ...range(1, 8).map((i) => [i, i] as const),
  ...range(1, 8).map((i) => [9 - i, i] as const),
];
