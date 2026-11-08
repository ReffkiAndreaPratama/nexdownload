import { useEffect } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import Layout from "./components/Layout";
import TokenGate from "./components/TokenGate";
import Analytics from "./pages/Analytics";
import Categories from "./pages/Categories";
import Collections from "./pages/Collections";
import Dashboard from "./pages/Dashboard";
import Diagnostics from "./pages/Diagnostics";
import Downloads from "./pages/Downloads";
import History from "./pages/History";
import Queues from "./pages/Queues";
import Rules from "./pages/Rules";
import Settings from "./pages/Settings";
import { useApp } from "./store/app";

export default function App() {
  const init = useApp((state) => state.init);
  const ready = useApp((state) => state.ready);
  const authorized = useApp((state) => state.authorized);

  useEffect(() => {
    void init();
  }, [init]);

  if (!ready) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <div className="flex flex-col items-center gap-4">
          <div className="relative h-12 w-12">
            <div className="absolute inset-0 rounded-full border-2 border-border" />
            <div className="absolute inset-0 animate-spin rounded-full border-2 border-transparent border-t-accent" />
          </div>
          <p className="text-sm text-muted">Starting NexDownload…</p>
        </div>
      </div>
    );
  }

  if (!authorized) {
    return <TokenGate />;
  }

  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<Dashboard />} />
        <Route path="downloads" element={<Downloads />} />
        <Route path="history" element={<History />} />
        <Route path="categories" element={<Categories />} />
        <Route path="queues" element={<Queues />} />
        <Route path="collections" element={<Collections />} />
        <Route path="rules" element={<Rules />} />
        <Route path="analytics" element={<Analytics />} />
        <Route path="diagnostics" element={<Diagnostics />} />
        <Route path="settings" element={<Settings />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
