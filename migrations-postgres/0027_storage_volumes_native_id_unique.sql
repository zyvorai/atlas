-- Copyright (c) 2026 ZyvorAI Labs Private Limited. All rights reserved.
CREATE UNIQUE INDEX IF NOT EXISTS idx_storage_volumes_native_id
    ON storage_volumes(backend_native_id) WHERE backend_native_id IS NOT NULL;
