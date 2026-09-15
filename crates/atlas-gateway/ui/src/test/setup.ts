// Copyright (c) 2026 ZyvorAI Labs Private Limited.
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
import "@testing-library/jest-dom/vitest";

// jsdom doesn't implement scroll APIs; several views call them from effects on mount
// (chapter navigation, scroll-to-top) purely for UX, so a no-op stub is enough to render.
Element.prototype.scrollTo = Element.prototype.scrollTo || (() => {});
Element.prototype.scrollIntoView = Element.prototype.scrollIntoView || (() => {});
window.scrollTo = window.scrollTo || (() => {});

// Chart libraries (recharts) measure their container via ResizeObserver, which jsdom lacks.
class ResizeObserverStub implements ResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
window.ResizeObserver = window.ResizeObserver || ResizeObserverStub;
