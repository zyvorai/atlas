// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
/** Shell look & feel — Apple shop dark / light. */
import type { Theme } from "../store/ui";

export const THEME_OPTIONS: { id: Theme; title: string; hint: string }[] = [
  { id: "carbon", title: "Carbon", hint: "Dark shop — black canvas + Apple Blue" },
  {
    id: "apple-lite",
    title: "Apple Lite",
    hint: "Light shop — #F5F5F7 canvas + Apple Blue",
  },
];

export function themeTitle(id: Theme): string {
  return THEME_OPTIONS.find((t) => t.id === id)?.title ?? id;
}
