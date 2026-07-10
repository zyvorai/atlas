-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
-- Day-2 maintenance: a cordoned backend rejects new provisioning (existing volumes are untouched),
-- so an operator can drain/quiesce a backend before maintenance without deleting anything.
ALTER TABLE storage_backends ADD COLUMN cordoned INTEGER NOT NULL DEFAULT 0;
