// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { Link, Route, Routes } from "react-router-dom";
import { Compass, Lock } from "lucide-react";
import { Shell } from "./shell/Shell";
import { APP_ROUTES, type NavModule } from "./nav/routes";
import { canModuleAccess } from "./nav/modules";
import { useUi } from "./store/ui";

function NotFound() {
  return (
    <div className="grid place-items-center py-24 text-center">
      <Compass size={32} className="text-muted-foreground mb-3" />
      <div className="text-lg font-semibold mb-1">Page not found</div>
      <div className="text-sm text-muted-foreground mb-4">There&apos;s nothing at this address.</div>
      <Link to="/" className="btn btn-primary">
        Back to Command Deck
      </Link>
    </div>
  );
}

function AccessDenied({ module }: { module: NavModule }) {
  const label = module.minRole === "admin" ? "Admin" : module.minRole === "operator" ? "Operator" : "Viewer";
  return (
    <div className="at-panel at-empty-box grid place-items-center py-20 text-center max-w-lg mx-auto mt-8">
      <Lock size={28} className="mb-3" style={{ color: "var(--at-ink-4)" }} aria-hidden />
      <div className="text-lg font-semibold mb-1">{label} access required</div>
      <p className="text-sm text-muted-foreground mb-4">
        Your session does not have permission to open <strong>{module.label}</strong>. Ask an admin to
        upgrade your role or use a token with {label.toLowerCase()} privileges.
      </p>
      <Link to="/" className="at-btn primary">
        Back to Command Deck
      </Link>
    </div>
  );
}

function GuardedRoute({ module }: { module: NavModule }) {
  const roleLevel = useUi((s) => s.roleLevel);
  if (!canModuleAccess(module, roleLevel)) return <AccessDenied module={module} />;
  const El = module.element!;
  return <El />;
}

export default function App() {
  return (
    <Routes>
      <Route element={<Shell />}>
        {APP_ROUTES.map((m) => (
          <Route key={m.id} path={m.path} element={<GuardedRoute module={m} />} />
        ))}
        <Route path="*" element={<NotFound />} />
      </Route>
    </Routes>
  );
}
