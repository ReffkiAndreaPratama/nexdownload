import { AlertTriangle, CheckCircle2, Info, X, XCircle } from "lucide-react";
import { cn } from "../lib/utils";
import { useApp, type ToastKind } from "../store/app";

const tones: Record<ToastKind, { icon: typeof Info; className: string }> = {
  success: { icon: CheckCircle2, className: "text-success" },
  error: { icon: XCircle, className: "text-danger" },
  warning: { icon: AlertTriangle, className: "text-warning" },
  info: { icon: Info, className: "text-info" },
};

export default function Toasts() {
  const toasts = useApp((state) => state.toasts);
  const dismiss = useApp((state) => state.dismissToast);

  if (toasts.length === 0) return null;

  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-[80] flex w-[min(92vw,380px)] flex-col gap-2">
      {toasts.map((toast) => {
        const { icon: Icon, className } = tones[toast.kind];
        return (
          <div
            key={toast.id}
            role="status"
            className={cn(
              "pointer-events-auto flex animate-slide-in items-start gap-3 rounded-xl border border-border bg-surface/95 p-3.5 shadow-2xl backdrop-blur",
            )}
          >
            <Icon className={cn("mt-0.5 h-4.5 w-4.5 shrink-0", className)} />
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium leading-tight">{toast.title}</p>
              {toast.message ? (
                <p className="mt-0.5 break-words text-xs leading-snug text-muted">
                  {toast.message}
                </p>
              ) : null}
            </div>
            <button
              type="button"
              className="rounded p-1 text-subtle transition hover:bg-raised hover:text-fg"
              onClick={() => dismiss(toast.id)}
              aria-label="Dismiss notification"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
