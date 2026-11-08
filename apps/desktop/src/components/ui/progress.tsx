import { cn } from "../../lib/utils";
import type { DownloadSegment } from "../../lib/types";

export function ProgressBar({
  value,
  segments,
  indeterminate,
  className,
}: {
  value: number;
  segments?: DownloadSegment[];
  indeterminate?: boolean;
  className?: string;
}) {
  if (indeterminate) {
    return (
      <div className={cn("relative h-1.5 w-full overflow-hidden rounded-full bg-raised", className)}>
        <div className="absolute inset-y-0 w-1/3 animate-indeterminate rounded-full bg-accent/80" />
      </div>
    );
  }

  if (segments && segments.length > 1) {
    const total = segments.reduce((sum, segment) => sum + (segment.end - segment.start + 1), 0);
    return (
      <div className={cn("flex h-1.5 w-full gap-[2px] overflow-hidden rounded-full", className)}>
        {segments.map((segment) => {
          const width = total > 0 ? ((segment.end - segment.start + 1) / total) * 100 : 0;
          const length = segment.end - segment.start + 1;
          const filled = length > 0 ? Math.min(100, (segment.downloaded / length) * 100) : 0;
          return (
            <div
              key={segment.index}
              className="relative h-full overflow-hidden rounded-full bg-raised"
              style={{ width: `${width}%` }}
              title={`Segment ${segment.index + 1}`}
            >
              <div
                className="h-full rounded-full bg-accent transition-[width] duration-500"
                style={{ width: `${filled}%` }}
              />
            </div>
          );
        })}
      </div>
    );
  }

  return (
    <div className={cn("h-1.5 w-full overflow-hidden rounded-full bg-raised", className)}>
      <div
        className="h-full rounded-full bg-accent transition-[width] duration-500"
        style={{ width: `${Math.min(100, Math.max(0, value))}%` }}
      />
    </div>
  );
}
