// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import LicenseBanner from "./LicenseBanner";
import { http } from "../api/client";

vi.mock("../api/client", () => ({
  http: { get: vi.fn() },
}));

// Regression coverage for the real bug this component shipped with: an expired token was
// initially indistinguishable from "no token" server-side (both trial_expired: false), which
// would have silently kept the "trial has ended" branch below from ever rendering. See
// crates/atlas-license/src/lib.rs's expired_status_is_never_confused_with_missing_status test
// for the backend half of this same regression guard.
describe("LicenseBanner", () => {
  const mockStatus = (data: object) => {
    (http.get as ReturnType<typeof vi.fn>).mockResolvedValue({ data });
  };

  it("renders nothing when licensed", async () => {
    mockStatus({
      licensed: true,
      trial_active: true,
      trial_expired: false,
      trial_days_remaining: 30,
      sales_contact: "sales@zyvor.dev",
    });
    const { container } = render(<LicenseBanner />);
    await waitFor(() => expect(http.get).toHaveBeenCalledWith("/license/status"));
    expect(container).toBeEmptyDOMElement();
  });

  it("renders the expired-trial message when trial_expired is true", async () => {
    mockStatus({
      licensed: false,
      trial_active: false,
      trial_expired: true,
      trial_days_remaining: 0,
      sales_contact: "sales@zyvor.dev",
    });
    render(<LicenseBanner />);
    await screen.findByRole("alert");
    expect(screen.getByText(/trial has ended/i)).toBeInTheDocument();
    expect(screen.getByText("sales@zyvor.dev")).toBeInTheDocument();
  });

  it("renders the days-remaining warning only when trial_days_remaining <= 7", async () => {
    mockStatus({
      licensed: false,
      trial_active: true,
      trial_expired: false,
      trial_days_remaining: 3,
      sales_contact: "sales@zyvor.dev",
    });
    render(<LicenseBanner />);
    const status = await screen.findByRole("status");
    expect(status).toHaveTextContent("3 days left in your Atlas trial");
  });

  it("renders nothing when more than 7 days remain", async () => {
    mockStatus({
      licensed: false,
      trial_active: true,
      trial_expired: false,
      trial_days_remaining: 20,
      sales_contact: "sales@zyvor.dev",
    });
    const { container } = render(<LicenseBanner />);
    await waitFor(() => expect(http.get).toHaveBeenCalledWith("/license/status"));
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing before the status response resolves", () => {
    (http.get as ReturnType<typeof vi.fn>).mockReturnValue(new Promise(() => {}));
    const { container } = render(<LicenseBanner />);
    expect(container).toBeEmptyDOMElement();
  });
});
