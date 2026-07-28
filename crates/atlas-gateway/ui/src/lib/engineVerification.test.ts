// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { describe, expect, it } from "vitest";
import { ENGINE_VERIFICATION, stageBadge, verificationFor } from "./engineVerification";

describe("engineVerification", () => {
  it("covers all six engines", () => {
    expect(ENGINE_VERIFICATION.map((e) => e.engine).sort()).toEqual(
      ["mariadb", "mongodb", "mysql", "oracle", "postgres", "sqlserver"].sort(),
    );
  });
  it("marks only Postgres CDC as live-verified", () => {
    expect(stageBadge(verificationFor("postgres"), "cdc")).toBe("live");
    expect(stageBadge(verificationFor("mysql"), "cdc")).toBe("pending");
    expect(stageBadge(verificationFor("oracle"), "full-load")).toBe("pending");
  });
});
