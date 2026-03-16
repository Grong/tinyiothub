-- Gateway Tables Migration
-- Creates tables for gateway auto-discovery protocol

-- Gateways table
CREATE TABLE IF NOT EXISTS gateways (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    token TEXT,
    token_expires_at TEXT,
    status TEXT DEFAULT 'offline' CHECK(status IN ('online', 'offline')),
    gateway_type TEXT,
    firmware_version TEXT,
    last_seen TEXT,
    api_key TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Gateway devices association table (flat structure)
CREATE TABLE IF NOT EXISTS gateway_devices (
    id TEXT PRIMARY KEY,
    gateway_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (gateway_id) REFERENCES gateways(id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    UNIQUE(gateway_id, device_id)
);

-- Indexes for gateways
CREATE INDEX IF NOT EXISTS idx_gateways_status ON gateways(status);
CREATE INDEX IF NOT EXISTS idx_gateways_api_key ON gateways(api_key);
CREATE INDEX IF NOT EXISTS idx_gateways_last_seen ON gateways(last_seen);

-- Indexes for gateway_devices
CREATE INDEX IF NOT EXISTS idx_gateway_devices_gateway ON gateway_devices(gateway_id);
CREATE INDEX IF NOT EXISTS idx_gateway_devices_device ON gateway_devices(device_id);

-- Sample gateway
INSERT INTO gateways (id, name, api_key, status, gateway_type, firmware_version) VALUES
('gw-001', 'Home Gateway', 'test-api-key-12345', 'offline', 'esp32-s3', '1.0.0');
