ALTER TABLE devices ADD COLUMN linked_gateway TEXT;
ALTER TABLE devices ADD COLUMN fingerprint TEXT;
CREATE INDEX IF NOT EXISTS idx_devices_linked_gateway ON devices(linked_gateway);
CREATE INDEX IF NOT EXISTS idx_devices_fingerprint ON devices(fingerprint);
