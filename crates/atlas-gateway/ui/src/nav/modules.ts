// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
// Re-exports from routes.ts for backward compatibility.
export type { NavModule as Module, SectionId } from "./routes";
export {
  MODULES,
  SECTIONS,
  SECTION_META,
  SPARK,
  modulesForRole,
  canModuleAccess,
  pinnedModules,
  activeModuleFromPath,
  navLabelForPath,
  sectionForPath,
  APP_ROUTES,
  shortcutTargets,
  moduleById,
  navCrumbs,
} from "./routes";
