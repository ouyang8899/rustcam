-- RustCam Database Schema
-- PostgreSQL 14+
-- Consolidated schema: run this script to initialize all database tables
-- This is the definitive schema synchronized with domain models

BEGIN;

-- ============================================
-- Users Table
-- ============================================
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL DEFAULT 'User',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);

-- ============================================
-- Device Groups Table
-- ============================================
CREATE TABLE IF NOT EXISTS device_groups (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    parent_id BIGINT REFERENCES device_groups(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_device_groups_parent ON device_groups(parent_id);

-- ============================================
-- Devices Table (Unified Device Model)
-- Supports both Devices (NVR, IPC, DVR) and Channels
-- Distinguish by: is_channel = true/false
-- ============================================
CREATE TABLE IF NOT EXISTS devices (
    id BIGSERIAL PRIMARY KEY,
    device_tag VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    protocol VARCHAR(50) NOT NULL,

    parent_device_tag VARCHAR(50),
    is_channel BOOLEAN DEFAULT FALSE,

    device_type VARCHAR(50) DEFAULT 'Other',
    device_type_code VARCHAR(10),

    host VARCHAR(255) DEFAULT '',
    port INTEGER DEFAULT 0,
    device_username VARCHAR(255),
    device_password VARCHAR(255),
    playback_username VARCHAR(255),
    playback_password VARCHAR(255),

    channel_id VARCHAR(50),
    channel_extended JSONB DEFAULT '{}',
    is_default BOOLEAN DEFAULT FALSE,

    status VARCHAR(50) NOT NULL DEFAULT 'offline',
    app VARCHAR(255),
    media_server_tag VARCHAR(255),
    region_code VARCHAR(100),
    is_public BOOLEAN NOT NULL DEFAULT FALSE,
    group_id BIGINT REFERENCES device_groups(id) ON DELETE SET NULL,
    extended JSONB DEFAULT '{}',
    push_urls JSONB DEFAULT '[]',
    pull_urls JSONB DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_devices_status ON devices(status);
CREATE INDEX IF NOT EXISTS idx_devices_group_id ON devices(group_id);
CREATE INDEX IF NOT EXISTS idx_devices_is_channel ON devices(is_channel);
CREATE INDEX IF NOT EXISTS idx_devices_device_type ON devices(device_type);
CREATE INDEX IF NOT EXISTS idx_devices_channel_id ON devices(channel_id);
CREATE INDEX IF NOT EXISTS idx_devices_parent_tag ON devices(parent_device_tag);
CREATE INDEX IF NOT EXISTS idx_devices_is_default ON devices(is_default);
CREATE INDEX IF NOT EXISTS idx_devices_channel_extended ON devices USING GIN (channel_extended);

-- ============================================
-- Servers Table (Media Servers)
-- ============================================
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
CREATE INDEX IF NOT EXISTS idx_servers_type ON servers(server_type);
CREATE INDEX IF NOT EXISTS idx_servers_enabled ON servers(enabled);

-- ============================================
-- Streams Table
-- ============================================
CREATE TABLE IF NOT EXISTS streams (
    id BIGSERIAL PRIMARY KEY,
    device_tag VARCHAR(255),
    channel_tag VARCHAR(255),
    media_server_tag VARCHAR(255) NOT NULL DEFAULT '',
    app VARCHAR(255) NOT NULL DEFAULT '',
    token VARCHAR(255) NOT NULL DEFAULT '',
    state VARCHAR(50) NOT NULL DEFAULT 'Idle',
    retry_count SMALLINT NOT NULL DEFAULT 0,
    max_retries SMALLINT NOT NULL DEFAULT 20,
    last_error TEXT,
    viewer_count INTEGER NOT NULL DEFAULT 0,
    bandwidth_in BIGINT NOT NULL DEFAULT 0,
    bandwidth_out BIGINT NOT NULL DEFAULT 0,
    last_keepalive_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_streams_device_tag ON streams(device_tag);
CREATE INDEX IF NOT EXISTS idx_streams_state ON streams(state);
CREATE INDEX IF NOT EXISTS idx_streams_token ON streams(token);

-- ============================================
-- Sessions Table
-- ============================================
CREATE TABLE IF NOT EXISTS sessions (
    id BIGSERIAL PRIMARY KEY,
    session_type VARCHAR(50) NOT NULL,
    user_id BIGINT NOT NULL,
    state VARCHAR(50) NOT NULL DEFAULT 'initializing',
    client_ip VARCHAR(255),
    client_type VARCHAR(100),
    media_server_tag VARCHAR(255),
    protocol VARCHAR(50),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_activity TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    bytes_sent BIGINT NOT NULL DEFAULT 0,
    bytes_received BIGINT NOT NULL DEFAULT 0,
    device_tag VARCHAR(255),
    channel_tag VARCHAR(255)
);
CREATE INDEX IF NOT EXISTS idx_sessions_device_tag ON sessions(device_tag);
CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state);
CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);

-- ============================================
-- Recordings Table
-- ============================================
CREATE TABLE IF NOT EXISTS recordings (
    id BIGSERIAL PRIMARY KEY,
    device_tag VARCHAR(255),
    channel_tag VARCHAR(255),
    media_server_name VARCHAR(255) NOT NULL,
    state VARCHAR(50) NOT NULL DEFAULT 'Starting',
    format VARCHAR(50) NOT NULL DEFAULT 'MP4',
    output_path TEXT,
    file_size BIGINT NOT NULL DEFAULT 0,
    duration_secs BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    stopped_at TIMESTAMPTZ,
    error_message TEXT,
    labels TEXT,
    filename TEXT
);
CREATE INDEX IF NOT EXISTS idx_recordings_device_tag ON recordings(device_tag);
CREATE INDEX IF NOT EXISTS idx_recordings_state ON recordings(state);
CREATE INDEX IF NOT EXISTS idx_recordings_created_at ON recordings(created_at);

-- ============================================
-- Regions Table
-- ============================================
CREATE TABLE IF NOT EXISTS regions (
    code VARCHAR(100) PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    level SMALLINT NOT NULL DEFAULT 0,
    parent_code VARCHAR(100) REFERENCES regions(code) ON DELETE CASCADE,
    province_name VARCHAR(255),
    city_name VARCHAR(255),
    district_name VARCHAR(255),
    gb28181_code VARCHAR(100) NOT NULL DEFAULT '',
    device_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_regions_parent ON regions(parent_code);
CREATE INDEX IF NOT EXISTS idx_regions_level ON regions(level);
CREATE INDEX IF NOT EXISTS idx_regions_gb28181 ON regions(gb28181_code);

-- ============================================
-- PTZ Presets Table
-- ============================================
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

-- ============================================
-- PTZ Control Logs Table
-- ============================================
CREATE TABLE IF NOT EXISTS ptz_control_logs (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT,
    device_id BIGINT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    command VARCHAR(100) NOT NULL,
    speed SMALLINT NOT NULL DEFAULT 50,
    result VARCHAR(50) NOT NULL DEFAULT 'success',
    error_message TEXT,
    call_id VARCHAR(100),
    sip_code INTEGER,
    device_response TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_ptz_logs_device_id ON ptz_control_logs(device_id);
CREATE INDEX IF NOT EXISTS idx_ptz_logs_user_id ON ptz_control_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_ptz_logs_created_at ON ptz_control_logs(created_at DESC);

-- ============================================
-- Player Window Layouts Table
-- ============================================
CREATE TABLE IF NOT EXISTS player_window_layouts (
    id SERIAL PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    rows INTEGER NOT NULL DEFAULT 2,
    cols INTEGER NOT NULL DEFAULT 2,
    layout_json JSONB NOT NULL DEFAULT '[]',
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_player_layouts_name ON player_window_layouts(name);

INSERT INTO player_window_layouts (name, rows, cols, layout_json, is_default)
VALUES (
    'Default 2x2',
    2, 2,
    '[{"id":"1","row":0,"col":0,"row_span":1,"col_span":1,"label":null},{"id":"2","row":0,"col":1,"row_span":1,"col_span":1,"label":null},{"id":"3","row":1,"col":0,"row_span":1,"col_span":1,"label":null},{"id":"4","row":1,"col":1,"row_span":1,"col_span":1,"label":null}]',
    TRUE
) ON CONFLICT DO NOTHING;

-- ============================================
-- Alarms Table
-- ============================================
CREATE TABLE IF NOT EXISTS alarms (
    id BIGSERIAL PRIMARY KEY,
    device_id BIGINT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    device_tag VARCHAR(64) NOT NULL,
    alarm_type VARCHAR(64) NOT NULL DEFAULT 'Alarm',
    alarm_time TIMESTAMP NOT NULL,
    alarm_method INT DEFAULT 1,
    alarm_priority INT DEFAULT 0,
    description TEXT,
    processed BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_alarms_device_time ON alarms(device_id, alarm_time DESC);
CREATE INDEX IF NOT EXISTS idx_alarms_processed ON alarms(processed) WHERE NOT processed;

-- ============================================
-- GB28181 Reference Data Tables
-- ============================================
CREATE TABLE IF NOT EXISTS gb_device_types (
    code VARCHAR(10) PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    name_en VARCHAR(100),
    category VARCHAR(50) NOT NULL,
    description TEXT,
    can_have_children BOOLEAN DEFAULT FALSE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS gb_industry_codes (
    code VARCHAR(10) PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    name_en VARCHAR(100),
    description TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS gb_network_codes (
    code VARCHAR(10) PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    name_en VARCHAR(100),
    description TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_gb_device_types_category ON gb_device_types(category);
CREATE INDEX IF NOT EXISTS idx_gb_device_types_sort ON gb_device_types(sort_order);
CREATE INDEX IF NOT EXISTS idx_gb_industry_codes_sort ON gb_industry_codes(sort_order);
CREATE INDEX IF NOT EXISTS idx_gb_network_codes_sort ON gb_network_codes(sort_order);

-- Insert GB28181 reference data
INSERT INTO gb_device_types (code, name, name_en, category, description, can_have_children, sort_order) VALUES
('111', '数字视频录像机', 'DVR', 'frontend_device', 'DVR', FALSE, 10),
('112', '视频服务器', 'Video Server', 'frontend_device', '视频服务器', FALSE, 11),
('113', '编码器', 'Encoder', 'frontend_device', '编码器', FALSE, 12),
('118', '网络视频录像机(NVR)', 'NVR', 'frontend_device', '网络视频录像机(NVR)', FALSE, 17),
('131', '摄像机', 'Camera', 'frontend_peripheral', '摄像机', FALSE, 30),
('132', '网络摄像机(IPC)', 'IPC', 'frontend_peripheral', '网络摄像机(IPC)', FALSE, 31),
('200', '中心信令控制服务器', 'Central Signaling', 'platform', '中心信令控制服务器', FALSE, 50),
('201', 'Web应用服务器', 'Web App Server', 'platform', 'Web应用服务器', FALSE, 51),
('500', '视频图像信息综合应用平台', 'Video Platform', 'platform_external', '视频图像信息综合应用平台', FALSE, 90)
ON CONFLICT (code) DO UPDATE SET
    name = EXCLUDED.name,
    name_en = EXCLUDED.name_en,
    category = EXCLUDED.category,
    description = EXCLUDED.description,
    can_have_children = EXCLUDED.can_have_children,
    sort_order = EXCLUDED.sort_order;

INSERT INTO gb_industry_codes (code, name, name_en, description, sort_order) VALUES
('00', '社会治安路面接入', 'Social Security Road Access', '社会治安路面接入', 0),
('01', '社会治安社区接入', 'Social Security Community Access', '社会治安社区接入', 1),
('02', '社会治安内部接入', 'Social Security Internal Access', '社会治安内部接入', 2)
ON CONFLICT (code) DO UPDATE SET
    name = EXCLUDED.name,
    name_en = EXCLUDED.name_en,
    description = EXCLUDED.description,
    sort_order = EXCLUDED.sort_order;

INSERT INTO gb_network_codes (code, name, name_en, description, sort_order) VALUES
('0', '公安视频传输网0', 'Police Video Network 0', '公安视频传输网0', 0),
('1', '公安视频传输网1', 'Police Video Network 1', '公安视频传输网1', 1),
('2', '行业专网', 'Industry Private Network', '行业专网', 2)
ON CONFLICT (code) DO UPDATE SET
    name = EXCLUDED.name,
    name_en = EXCLUDED.name_en,
    description = EXCLUDED.description,
    sort_order = EXCLUDED.sort_order;

COMMIT;
