// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
/** Shell look & feel options — mirrors Zeus metal shells (dark-steel / zinc-metal) + Soundings. */
import type { Theme } from "../store/ui";

export const THEME_OPTIONS: { id: Theme; title: string; hint: string }[] = [
  { id: "nebula", title: "Nebula", hint: "Soundings cyan — Atlas default" },
  { id: "dark", title: "Dark steel", hint: "Metal top bar, cool accents" },
  { id: "zinc", title: "Zinc metal", hint: "Brushed zinc, amber highlights" },
  { id: "aurora", title: "Aurora", hint: "Neon cyan / violet glow" },
];

export function themeTitle(id: Theme): string {
  return THEME_OPTIONS.find((t) => t.id === id)?.title ?? id;
}
