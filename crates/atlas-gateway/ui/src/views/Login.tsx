// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
// Premium branded login / entry gate (Zyvor identity), styled with the vendored design foundation.
import { useEffect, useState } from "react";
import { ArrowRight, Hexagon, KeyRound, ShieldCheck } from "lucide-react";
import { useUi } from "../store/ui";
import { Button, Field } from "../ui/kit";

export function Login() {
  const enter = useUi((s) => s.enter);
  const setToken = useUi((s) => s.setToken);
  const [token, setDraft] = useState(useUi.getState().token);
  const [ver, setVer] = useState("");
  const [health, setHealth] = useState<"ok" | "down" | "…">("…");
  useEffect(() => {
    fetch("/version").then((r) => r.json()).then((v) => setVer(v.version)).catch(() => {});
    fetch("/health").then((r) => setHealth(r.ok ? "ok" : "down")).catch(() => setHealth("down"));
  }, []);
  const go = () => {
    if (token.trim()) setToken(token.trim());
    enter();
  };
  return (
    <div className="login-shell h-full w-full grid place-items-center p-6 overflow-hidden">
      <div className="login-orbs" />
      <div className="relative w-full max-w-[420px] glass-card p-8 animate-fade-in">
        <div className="flex flex-col items-center text-center mb-6">
          <div className="w-16 h-16 rounded-2xl grid place-items-center mb-4 login-mark">
            <Hexagon size={30} className="text-white" fill="currentColor" />
          </div>
          <div className="text-2xl font-extrabold tracking-tight">
            <span className="bg-gradient-to-r from-sky-400 to-blue-500 bg-clip-text text-transparent">ATLAS</span>
          </div>
          <div className="text-sm text-muted-foreground mt-1">Zyvor Storage Control Plane</div>
        </div>

        <div className="text-xs text-muted-foreground mb-2 flex items-center gap-1.5">
          <KeyRound size={13} /> Service-account token <span className="opacity-60">(optional — required only when auth is enforced)</span>
        </div>
        <Field placeholder="eyJhbGciOi…" value={token} onChange={(e) => setDraft(e.target.value)} onKeyDown={(e) => e.key === "Enter" && go()} />

        <Button variant="primary" className="w-full mt-5 justify-center" onClick={go}>
          Enter Storage Center <ArrowRight size={15} />
        </Button>

        <div className="flex items-center justify-between mt-6 text-[11px] text-muted-foreground/70">
          <span className="flex items-center gap-1.5">
            <span className={`w-1.5 h-1.5 rounded-full ${health === "ok" ? "bg-success" : health === "down" ? "bg-danger" : "bg-muted"}`} />
            gateway {health}{ver && ` · v${ver}`}
          </span>
          <a href="https://zyvor.dev" target="_blank" rel="noreferrer" className="flex items-center gap-1 hover:text-sky-300">
            <ShieldCheck size={12} /> zyvor.dev
          </a>
        </div>
      </div>
      <div className="absolute bottom-4 text-[11px] text-muted-foreground/50">© 2026 ZyvorAI Labs Private Limited · Atlas</div>
    </div>
  );
}
