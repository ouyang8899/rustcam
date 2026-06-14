-- RustCam-Media Database Schema
-- Run this script to initialize the database tables

-- Device Groups table
CREATE TABLE IF NOT EXISTS device_groups (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    parent_id BIGINT REFERENCES device_groups(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_device_groups_parent ON device_groups(parent_id);

-- Devices table
CREATE TABLE IF NOT EXISTS devices (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    protocol VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'offline',
    host VARCHAR(255) DEFAULT '',
    port INTEGER DEFAULT 0,
    rtsp_url TEXT,
    device_tag VARCHAR(255),
    parent_device_tag VARCHAR(255),
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    group_id BIGINT,
    extended JSONB
);

CREATE INDEX IF NOT EXISTS idx_devices_status ON devices(status);
CREATE INDEX IF NOT EXISTS idx_devices_group_id ON devices(group_id);

-- Servers table
CREATE TABLE IF NOT EXISTS servers (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    url TEXT NOT NULL,
    api_key TEXT NOT NULL,
    server_type VARCHAR(50) NOT NULL,
    weight INTEGER NOT NULL DEFAULT 100,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    server_tag VARCHAR(255) NOT NULL DEFAULT '',
    protocol_ports JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Streams table
CREATE TABLE IF NOT EXISTS streams (
    id BIGSERIAL PRIMARY KEY,
    device_id BIGINT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    session_id BIGINT,
    media_server_id VARCHAR(255),
    stream_key VARCHAR(255),
    token VARCHAR(255),
    state VARCHAR(50) NOT NULL DEFAULT 'idle',
    viewer_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_streams_device_id ON streams(device_id);

-- Sessions table
CREATE TABLE IF NOT EXISTS sessions (
    id BIGSERIAL PRIMARY KEY,
    session_type VARCHAR(50) NOT NULL,
    device_id BIGINT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL,
    state VARCHAR(50) NOT NULL DEFAULT 'initializing',
    client_ip VARCHAR(255),
    client_type VARCHAR(100),
    media_server_id VARCHAR(255),
    stream_id VARCHAR(255) NOT NULL DEFAULT '',
    protocol VARCHAR(50),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_activity TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    bytes_sent BIGINT NOT NULL DEFAULT 0,
    bytes_received BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_sessions_device_id ON sessions(device_id);
CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state);

-- Recordings table
CREATE TABLE IF NOT EXISTS recordings (
    id BIGSERIAL PRIMARY KEY,
    device_id BIGINT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    stream_key VARCHAR(255) NOT NULL,
    media_server_name VARCHAR(255) NOT NULL,
    state VARCHAR(50) NOT NULL DEFAULT 'starting',
    format VARCHAR(50) NOT NULL,
    output_path TEXT,
    file_size BIGINT NOT NULL DEFAULT 0,
    duration_secs BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    stopped_at TIMESTAMPTZ,
    error_message TEXT,
    labels TEXT
);

CREATE INDEX IF NOT EXISTS idx_recordings_device_id ON recordings(device_id);
CREATE INDEX IF NOT EXISTS idx_recordings_state ON recordings(state);

-- PTZ Presets table
CREATE TABLE IF NOT EXISTS ptz_presets (
    id BIGSERIAL PRIMARY KEY,
    device_id BIGINT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    token VARCHAR(255) NOT NULL,
    position_pan DOUBLE PRECISION,
    position_tilt DOUBLE PRECISION,
    position_zoom DOUBLE PRECISION,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(device_id, token)
);

CREATE INDEX IF NOT EXISTS idx_ptz_presets_device_id ON ptz_presets(device_id);

-- PTZ Control Logs table
CREATE TABLE IF NOT EXISTS ptz_control_logs (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT,
    device_id BIGINT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    command VARCHAR(100) NOT NULL,
    speed INTEGER NOT NULL DEFAULT 50,
    result VARCHAR(50) NOT NULL DEFAULT 'success',
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ptz_logs_device_id ON ptz_control_logs(device_id);
