// Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
import { BookOpen, Boxes, ExternalLink, type LucideIcon } from "lucide-react";

export const ZYVOR_URL = "https://zyvor.dev";

export interface SuiteLink {
  id: string;
  label: string;
  href: string;
  icon: LucideIcon;
}

export const SUITE_LINKS: SuiteLink[] = [
  { id: "zeus-os", label: "Zeus OS", href: `${ZYVOR_URL}/zeus-os`, icon: Boxes },
  { id: "zyvor", label: "zyvor.dev", href: ZYVOR_URL, icon: ExternalLink },
  {
    id: "atlas-docs",
    label: "Atlas docs",
    href: `${ZYVOR_URL}/docs/atlas`,
    icon: BookOpen,
  },
];
