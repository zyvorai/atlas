// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
import { describe, expect, it } from "vitest";
import { isModuleActive } from "./useNavGroups";

describe("useNavGroups helpers", () => {
  const mod = {
    id: "volumes",
    codename: "atlas",
    label: "Volumes",
    path: "/volumes",
    icon: (() => null) as never,
    section: "STORAGE" as const,
  };

  it("isModuleActive matches exact path and nested paths", () => {
    expect(isModuleActive(mod, "/volumes")).toBe(true);
    expect(isModuleActive(mod, "/volumes/extra")).toBe(true);
    expect(isModuleActive(mod, "/volume")).toBe(false);
  });
});
