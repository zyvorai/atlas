// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
/** Shell look & feel options — ported 1:1 from Zeus OS's two shipped shells. */
import type { Theme } from "../store/ui";

export const THEME_OPTIONS: { id: Theme; title: string; hint: string }[] = [
  { id: "carbon", title: "Carbon", hint: "Graphite + Cosmic Orange — Zeus OS default dark shell" },
  {
    id: "apple-lite",
    title: "Apple Lite",
    hint: "iPhone 17 Magichromatic — Mist Blue · Sage · Lavender",
  },
];

export function themeTitle(id: Theme): string {
  return THEME_OPTIONS.find((t) => t.id === id)?.title ?? id;
}
