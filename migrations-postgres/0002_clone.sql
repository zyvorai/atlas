-- Copyright (c) 2026 ZyvorAI Labs Private Limited.
-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
-- Clone/restore dependency tracking: a volume may be provisioned from a snapshot.

ALTER TABLE storage_volumes ADD COLUMN source_snapshot_id TEXT;

CREATE INDEX IF NOT EXISTS idx_volumes_source_snapshot
    ON storage_volumes(source_snapshot_id);
