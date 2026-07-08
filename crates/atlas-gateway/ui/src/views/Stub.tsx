// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { Sparkles } from "lucide-react";
import { PageHeader } from "../ui/kit";

export default function Stub({ title }: { title: string }) {
  return (
    <div>
      <PageHeader icon={Sparkles} title={title} subtitle="Wiring in progress" />
      <div className="glass-card p-10 text-center text-muted-foreground">This Center is being wired to the Atlas API.</div>
    </div>
  );
}
