-- Copyright (c) 2026 ZyvorAI Labs Private Limited.
-- SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Atlas-Commercial
CREATE TABLE IF NOT EXISTS rbd_snapshots (
    pool       TEXT NOT NULL,
    image      TEXT NOT NULL,
    snap       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')),
    PRIMARY KEY (pool, image, snap)
);
