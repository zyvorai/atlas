// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
import { afterEach, describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import { flushQueries, renderView, stubFetch } from "../test/renderView";
import { Login } from "./Login";

describe("Login", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders the sign-in form without a live gateway", async () => {
    stubFetch();
    renderView(<Login />);
    await flushQueries();
    expect(screen.getByLabelText("Account")).toBeInTheDocument();
    expect(screen.getByLabelText("Username")).toBeInTheDocument();
  });
});
