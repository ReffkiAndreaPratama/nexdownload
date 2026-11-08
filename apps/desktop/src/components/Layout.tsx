import { useEffect, useState } from "react";
import { NavLink, Outlet, useLocation } from "react-router-dom";
import {
  Activity,
  BarChart3,
  BookMarked,
  Download,
  FolderTree,
  History,
  LayoutDashboard,
  ListOrdered,
  Menu,
  Plus,
  Settings,
  Sparkles,
  Stethoscope,
  X,
} from "lucide-react";
import { isTauri } from "../lib/api";
import { cn } from "../lib/utils";
import { useApp } from "../store/app";
import AddDownloadDialog from "./AddDownloadDialog";
import TaskDetail from "./TaskDetail";
import Toasts from "./Toasts";

const navGroups = [
  {
    label: "Workspace",
    items: [
      { to: "/", label: "Dashboard", icon: LayoutDashboard, end: true },
      { to: "/downloads", label: "Downloads", icon: Download },
      { to: "/history", label: "History", icon: History },
    ],
  },
  {
    label: "Organize",
    items: [
      { to: "/categories", label: "Categories", icon: FolderTree },
      { to: "/queues", label: "Queues", icon: ListOrdered },
      { to: "/collections", label: "Collections", icon: BookMarked },
      { to: "/rules", label: "Rules", icon: Sparkles },
    ],
  },
  {
    label: "Insights",
    items: [
      { to: "/analytics", label: "Analytics", icon: BarChart3 },
      { to: "/diagnostics", label: "Diagnostics", icon: Stethoscope },
    ],
  },
];

const titles: Record<string, string> = {
  "/": "Dashboard",
  "/downloads": "Downloads",
  "/history": "History",
  "/categories": "Categories",
  "/queues": "Queues",
  "/collections": "Collections",
  "/rules": "Rules",
  "/analytics": "Analytics",
  "/diagnostics": "Diagnostics",
  "/settings": "Settings",
};

function Sidebar({ onNavigate }: { onNavigate?: () => void }) {
  const tasks = useApp((state) => state.tasks);
  const baseUrl = useApp((state) => state.baseUrl);
  const active = tasks.filter((task) =>
    ["downloading", "connecting", "preparing", "queued"].includes(task.status),
  ).length;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2.5 px-5 py-5">
        <div className="brand-mark">
          <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" aria-hidden="true">
            <path d="M6 19V5h3l7.5 9.5V5H20v14h-3.2L9.5 9.2V19H6Z" fill="currentColor" />
            <path d="M6 21h12" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
          </svg>
        </div>
        <div className="min-w-0">
          <p className="text-sm font-semibold tracking-tight">NexDownload</p>
          <p className="text-[10px] uppercase tracking-[0.14em] text-subtle">Self-hosted engine</p>
        </div>
      </div>

      <nav className="flex-1 space-y-5 overflow-y-auto px-3 pb-4 scroll-thin">
        {navGroups.map((group) => (
          <div key={group.label}>
            <p className="mb-1.5 px-2 text-[10px] font-semibold uppercase tracking-[0.13em] text-subtle">
              {group.label}
            </p>
            <div className="space-y-0.5">
              {group.items.map((item) => {
                const Icon = item.icon;
                return (
                  <NavLink
                    key={item.to}
                    to={item.to}
                    end={item.end}
                    onClick={onNavigate}
                    className={({ isActive }) =>
                      cn(
                        "group flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-[13px] font-medium transition",
                        isActive
                          ? "bg-accent/10 text-accent"
                          : "text-muted hover:bg-raised hover:text-fg",
                      )
                    }
                  >
                    <Icon className="h-4 w-4 shrink-0" />
                    <span className="flex-1">{item.label}</span>
                    {item.label === "Downloads" && active > 0 ? (
                      <span className="rounded-full bg-accent/15 px-1.5 py-0.5 text-[10px] font-semibold text-accent">
                        {active}
                      </span>
                    ) : null}
                  </NavLink>
                );
              })}
            </div>
          </div>
        ))}
      </nav>

      <div className="border-t border-border p-3">
        <NavLink
          to="/settings"
          onClick={onNavigate}
          className={({ isActive }) =>
            cn(
              "flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-[13px] font-medium transition",
              isActive ? "bg-accent/10 text-accent" : "text-muted hover:bg-raised hover:text-fg",
            )
          }
        >
          <Settings className="h-4 w-4" />
          Settings
        </NavLink>
        <div className="mt-2 rounded-lg bg-bg/50 px-2.5 py-2">
          <p className="flex items-center gap-1.5 text-[10px] font-medium text-success">
            <span className="h-1.5 w-1.5 rounded-full bg-success" /> Engine connected
          </p>
          <p className="mt-1 truncate font-mono text-[10px] text-subtle" title={baseUrl}>
            {baseUrl.replace(/^https?:\/\//, "")}
          </p>
          <p className="mt-0.5 text-[10px] text-subtle">{isTauri ? "Desktop shell" : "Web client"}</p>
        </div>
      </div>
    </div>
  );
}

export default function Layout() {
  const location = useLocation();
  const setAddOpen = useApp((state) => state.setAddOpen);
  const refreshLive = useApp((state) => state.refreshLive);
  const refreshMeta = useApp((state) => state.refreshMeta);
  const authorized = useApp((state) => state.authorized);
  const [mobileOpen, setMobileOpen] = useState(false);

  useEffect(() => {
    if (!authorized) return;
    void refreshLive();
    const live = window.setInterval(() => void refreshLive(), 1200);
    const meta = window.setInterval(() => void refreshMeta(), 15000);
    return () => {
      window.clearInterval(live);
      window.clearInterval(meta);
    };
  }, [authorized, refreshLive, refreshMeta]);

  useEffect(() => {
    setMobileOpen(false);
  }, [location.pathname]);

  const title = titles[location.pathname] ?? "NexDownload";

  return (
    <div className="min-h-screen">
      <aside className="fixed inset-y-0 left-0 z-30 hidden w-60 border-r border-border bg-surface/60 backdrop-blur lg:block">
        <Sidebar />
      </aside>

      {mobileOpen ? (
        <div className="fixed inset-0 z-50 lg:hidden">
          <div
            className="absolute inset-0 bg-black/60 backdrop-blur-sm"
            onClick={() => setMobileOpen(false)}
          />
          <aside className="absolute inset-y-0 left-0 w-64 animate-slide-in border-r border-border bg-surface shadow-2xl">
            <button
              type="button"
              className="icon-btn absolute right-3 top-4 z-10"
              onClick={() => setMobileOpen(false)}
              aria-label="Close navigation"
            >
              <X className="h-4 w-4" />
            </button>
            <Sidebar onNavigate={() => setMobileOpen(false)} />
          </aside>
        </div>
      ) : null}

      <div className="lg:pl-60">
        <header className="sticky top-0 z-20 flex h-14 items-center gap-3 border-b border-border/80 bg-bg/80 px-4 backdrop-blur-md sm:px-6">
          <button
            type="button"
            className="icon-btn lg:hidden"
            onClick={() => setMobileOpen(true)}
            aria-label="Open navigation"
          >
            <Menu className="h-5 w-5" />
          </button>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-semibold">{title}</p>
          </div>
          <div className="hidden items-center gap-1.5 rounded-full border border-border bg-surface/70 px-2.5 py-1 text-[11px] text-muted sm:flex">
            <Activity className="h-3 w-3 text-accent" />
            Live polling
          </div>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={() => setAddOpen(true)}
          >
            <Plus className="h-3.5 w-3.5" /> New download
          </button>
        </header>

        <main className="mx-auto w-full max-w-[1500px] px-4 pb-16 pt-6 sm:px-6 lg:px-8">
          <Outlet />
        </main>
      </div>

      <TaskDetail />
      <AddDownloadDialog />
      <Toasts />
    </div>
  );
}
