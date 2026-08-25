// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { useEffect, useState } from "react";
import { http } from "../api/client";

interface LicenseStatus {
  licensed: boolean;
  trial_active: boolean;
  trial_expired: boolean;
  trial_days_remaining: number;
  licensee?: string | null;
  sales_contact: string;
}

// Mirrors Aurora's LicenseBanner.tsx / gtm_api.middleware.license — same product-family
// trial/license design (see docs/LICENSING.md), server-verified via GET /license/status
// (crates/atlas-gateway/src/license.rs), which stays reachable even once expired.
export default function LicenseBanner() {
  const [status, setStatus] = useState<LicenseStatus | null>(null);

  useEffect(() => {
    let cancelled = false;
    http
      .get<LicenseStatus>("/license/status")
      .then((res) => {
        if (!cancelled) setStatus(res.data);
      })
      .catch(() => {
        /* status endpoint itself never gates — a network hiccup just skips the banner */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!status || status.licensed) return null;
  if (!status.trial_active && !status.trial_expired) return null;

  const contact = status.sales_contact || "sales@zyvor.dev";

  if (status.trial_expired) {
    return (
      <div
        role="alert"
        className="w-full px-4 py-2.5 text-center text-sm text-danger-foreground bg-danger"
      >
        Your Atlas trial has ended. Email{" "}
        <a className="underline underline-offset-2" href={`mailto:${contact}`}>
          {contact}
        </a>{" "}
        for a license key to continue.
      </div>
    );
  }

  if (status.trial_days_remaining > 7) return null;

  return (
    <div
      role="status"
      className="w-full px-4 py-2 text-center text-sm text-warning-foreground bg-warning"
    >
      {status.trial_days_remaining} day{status.trial_days_remaining === 1 ? "" : "s"} left in your
      Atlas trial — email{" "}
      <a className="underline underline-offset-2" href={`mailto:${contact}`}>
        {contact}
      </a>{" "}
      for a license key.
    </div>
  );
}
