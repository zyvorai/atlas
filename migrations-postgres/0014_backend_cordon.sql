-- Copyright (c) 2026 ZyvorAI Labs Private Limited.
-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
-- Day-2 maintenance: a cordoned backend rejects new provisioning (existing volumes are untouched),
-- so an operator can drain/quiesce a backend before maintenance without deleting anything.
ALTER TABLE storage_backends ADD COLUMN cordoned INTEGER NOT NULL DEFAULT 0;
