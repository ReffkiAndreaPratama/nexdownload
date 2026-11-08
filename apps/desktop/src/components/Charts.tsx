import { useId } from "react";
import { cn } from "../lib/utils";

export function Sparkline({
  values,
  className,
  height = 64,
}: {
  values: number[];
  className?: string;
  height?: number;
}) {
  const gradientId = useId();
  if (values.length === 0) {
    return (
      <div
        className={cn("flex items-center justify-center rounded-lg border border-dashed border-border text-xs text-subtle", className)}
        style={{ height }}
      >
        No samples yet
      </div>
    );
  }

  const width = 600;
  const max = Math.max(...values, 1);
  const min = Math.min(...values, 0);
  const range = max - min || 1;
  const step = values.length > 1 ? width / (values.length - 1) : width;
  const points = values.map((value, index) => {
    const x = index * step;
    const y = height - ((value - min) / range) * (height - 8) - 4;
    return `${x.toFixed(2)},${y.toFixed(2)}`;
  });
  const line = points.join(" ");
  const area = `0,${height} ${line} ${width},${height}`;

  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      className={cn("w-full", className)}
      style={{ height }}
      role="img"
      aria-label="Download speed over time"
    >
      <defs>
        <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="hsl(var(--accent))" stopOpacity="0.35" />
          <stop offset="100%" stopColor="hsl(var(--accent))" stopOpacity="0" />
        </linearGradient>
      </defs>
      <polygon points={area} fill={`url(#${gradientId})`} />
      <polyline
        points={line}
        fill="none"
        stroke="hsl(var(--accent))"
        strokeWidth="2"
        strokeLinejoin="round"
        strokeLinecap="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

export function BarChart({
  data,
  className,
}: {
  data: { label: string; value: number; hint?: string }[];
  className?: string;
}) {
  if (data.length === 0) {
    return (
      <div className={cn("flex h-40 items-center justify-center text-xs text-subtle", className)}>
        No data yet
      </div>
    );
  }
  const max = Math.max(...data.map((item) => item.value), 1);

  return (
    <div className={cn("flex h-40 items-end gap-1.5", className)}>
      {data.map((item) => (
        <div key={item.label} className="group flex min-w-0 flex-1 flex-col items-center gap-1">
          <div className="relative flex w-full flex-1 items-end">
            <div
              className="w-full rounded-t bg-accent/70 transition group-hover:bg-accent"
              style={{ height: `${Math.max(2, (item.value / max) * 100)}%` }}
              title={item.hint ?? `${item.label}: ${item.value}`}
            />
          </div>
          <span className="w-full truncate text-center text-[10px] text-subtle">{item.label}</span>
        </div>
      ))}
    </div>
  );
}

export function DistributionBar({
  items,
}: {
  items: { label: string; value: number; color?: string }[];
}) {
  const total = items.reduce((sum, item) => sum + item.value, 0);
  if (total <= 0) {
    return <p className="text-xs text-subtle">No downloads recorded yet.</p>;
  }
  const palette = ["bg-accent", "bg-info", "bg-success", "bg-warning", "bg-danger", "bg-muted"];
  return (
    <div className="space-y-3">
      <div className="flex h-2.5 w-full overflow-hidden rounded-full bg-raised">
        {items.map((item, index) => (
          <div
            key={item.label}
            className={item.color ?? palette[index % palette.length]}
            style={{ width: `${(item.value / total) * 100}%` }}
            title={`${item.label}: ${item.value}`}
          />
        ))}
      </div>
      <div className="flex flex-wrap gap-x-4 gap-y-1.5">
        {items.map((item, index) => (
          <span key={item.label} className="inline-flex items-center gap-1.5 text-xs text-muted">
            <span className={cn("h-2 w-2 rounded-full", item.color ?? palette[index % palette.length])} />
            {item.label}
            <span className="text-subtle">{item.value}</span>
          </span>
        ))}
      </div>
    </div>
  );
}
