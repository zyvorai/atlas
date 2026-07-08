-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
-- Track whether an open alert has already been pushed to the notification webhook, so the monitor
-- fires exactly once per firing (and re-fires when a resolved alert re-opens).
ALTER TABLE storage_alerts ADD COLUMN notified_at TEXT;
