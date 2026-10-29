-- RustCam Database Schema
-- 由运行中的数据库导出（sql/export_schema.py），执行本脚本可初始化全部表结构。
-- 导出自 PostgreSQL 18.4

BEGIN;

-- ============================================
-- Extensions
-- ============================================
CREATE EXTENSION IF NOT EXISTS postgis WITH SCHEMA public;  -- 3.6.2

-- ============================================
-- Sequences
-- ============================================
CREATE SEQUENCE IF NOT EXISTS alarms_id_seq;
CREATE SEQUENCE IF NOT EXISTS channels_id_seq;
CREATE SEQUENCE IF NOT EXISTS device_groups_id_seq;
CREATE SEQUENCE IF NOT EXISTS devices_id_seq;
CREATE SEQUENCE IF NOT EXISTS player_window_layouts_id_seq;
CREATE SEQUENCE IF NOT EXISTS ptz_control_logs_id_seq;
CREATE SEQUENCE IF NOT EXISTS ptz_presets_id_seq;
CREATE SEQUENCE IF NOT EXISTS recording_policies_id_seq;
CREATE SEQUENCE IF NOT EXISTS recording_segments_id_seq;
CREATE SEQUENCE IF NOT EXISTS recording_time_windows_id_seq;
CREATE SEQUENCE IF NOT EXISTS recordings_id_seq;
CREATE SEQUENCE IF NOT EXISTS servers_id_seq;
CREATE SEQUENCE IF NOT EXISTS sessions_id_seq;
CREATE SEQUENCE IF NOT EXISTS streams_id_seq;

-- ============================================
-- Table: alarms
-- ============================================
CREATE TABLE IF NOT EXISTS alarms (
    id bigint DEFAULT nextval('alarms_id_seq'::regclass) NOT NULL,
    device_id bigint NOT NULL,
    device_tag varchar(64) NOT NULL,
    channel_tag varchar(64),
    alarm_type varchar(64) DEFAULT 'Alarm'::character varying NOT NULL,
    alarm_time timestamp NOT NULL,
    alarm_method integer DEFAULT 1,
    alarm_priority integer DEFAULT 0,
    description text,
    processed boolean DEFAULT false,
    created_at timestamp DEFAULT now(),
    CONSTRAINT alarms_pkey PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_alarms_device_time ON alarms USING btree (device_id, alarm_time DESC);
CREATE INDEX IF NOT EXISTS idx_alarms_channel_time ON alarms USING btree (channel_tag, alarm_time DESC);
CREATE INDEX IF NOT EXISTS idx_alarms_processed ON alarms USING btree (processed) WHERE (NOT processed);

-- ============================================
-- Table: channels
-- ============================================
CREATE TABLE IF NOT EXISTS channels (
    id bigint DEFAULT nextval('channels_id_seq'::regclass) NOT NULL,
    device_tag varchar(50) NOT NULL,
    channel_tag varchar(50) NOT NULL,
    name varchar(255) DEFAULT ''::character varying NOT NULL,
    status varchar(50) DEFAULT 'offline'::character varying NOT NULL,
    device_type varchar(50) DEFAULT 'Other'::character varying,
    device_type_code varchar(10),
    channel_extended jsonb DEFAULT '{}'::jsonb,
    is_default boolean DEFAULT false,
    parent_channel_tag varchar(50),
    civil_code varchar(100),
    address varchar(500),
    ip_address varchar(50),
    port integer DEFAULT 0,
    manufacturer varchar(255),
    model varchar(255),
    parental integer DEFAULT 0,
    latitude double precision,
    longitude double precision,
    geom geometry(Point,4326),
    extended jsonb DEFAULT '{}'::jsonb,
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT channels_pkey PRIMARY KEY (id),
    CONSTRAINT channels_device_tag_channel_tag_key UNIQUE (device_tag, channel_tag)
);
CREATE UNIQUE INDEX IF NOT EXISTS channels_device_tag_channel_tag_key ON channels USING btree (device_tag, channel_tag);
CREATE INDEX IF NOT EXISTS idx_channels_channel_extended ON channels USING gin (channel_extended);
CREATE INDEX IF NOT EXISTS idx_channels_channel_tag ON channels USING btree (channel_tag);
CREATE INDEX IF NOT EXISTS idx_channels_device_tag ON channels USING btree (device_tag);
CREATE INDEX IF NOT EXISTS idx_channels_is_default ON channels USING btree (is_default);
CREATE INDEX IF NOT EXISTS idx_channels_parent_channel ON channels USING btree (parent_channel_tag);
CREATE INDEX IF NOT EXISTS idx_channels_status ON channels USING btree (status);
CREATE INDEX IF NOT EXISTS idx_channels_geom ON channels USING GIST (geom);
CREATE INDEX IF NOT EXISTS idx_streams_channel_tag ON channels USING btree (channel_tag);

-- ============================================
-- Table: device_groups
-- ============================================
CREATE TABLE IF NOT EXISTS device_groups (
    id bigint DEFAULT nextval('device_groups_id_seq'::regclass) NOT NULL,
    name varchar(255) NOT NULL,
    parent_id bigint,
    sort_order integer DEFAULT 0 NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT device_groups_pkey PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_device_groups_parent ON device_groups USING btree (parent_id);

-- ============================================
-- Table: devices
-- ============================================
CREATE TABLE IF NOT EXISTS devices (
    id bigint DEFAULT nextval('devices_id_seq'::regclass) NOT NULL,
    name varchar(255) NOT NULL,
    protocol varchar(50) NOT NULL,
    status varchar(50) DEFAULT 'Offline'::character varying NOT NULL,
    rtsp_url text,
    is_public boolean DEFAULT false NOT NULL,
    view_count bigint DEFAULT 0 NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    last_seen timestamptz,
    host varchar(255) DEFAULT ''::character varying NOT NULL,
    port integer DEFAULT 554 NOT NULL,
    extended jsonb,
    device_password varchar(255),
    playback_username varchar(255),
    playback_password varchar(255),
    media_server_tag varchar(255),
    push_urls jsonb DEFAULT '[]'::jsonb,
    pull_urls jsonb DEFAULT '[]'::jsonb,
    region_code varchar(255),
    updated_at timestamptz DEFAULT now() NOT NULL,
    device_tag varchar(255),
    parent_device_tag varchar(255),
    ssrc varchar(255),
    group_id bigint,
    gb_username varchar(255),
    gb_password varchar(255),
    device_id varchar(255),
    stream_key varchar(255),
    device_username varchar(255),
    app varchar(255),
    device_type varchar(50) DEFAULT 'Other'::character varying,
    device_type_code varchar(10),
    channel_count integer DEFAULT 0,
    CONSTRAINT devices_pkey PRIMARY KEY (id)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_devices_device_id ON devices USING btree (device_id) WHERE (device_id IS NOT NULL);
CREATE INDEX IF NOT EXISTS idx_devices_group_id ON devices USING btree (group_id);
CREATE INDEX IF NOT EXISTS idx_devices_is_public ON devices USING btree (is_public);
CREATE INDEX IF NOT EXISTS idx_devices_parent ON devices USING btree (parent_device_tag) WHERE (parent_device_tag IS NOT NULL);
CREATE INDEX IF NOT EXISTS idx_devices_protocol ON devices USING btree (protocol);
CREATE INDEX IF NOT EXISTS idx_devices_region ON devices USING btree (((extended ->> 'region_code'::text)));
CREATE INDEX IF NOT EXISTS idx_devices_status ON devices USING btree (status);

-- ============================================
-- Table: gb28181_regions
-- ============================================
CREATE TABLE IF NOT EXISTS gb28181_regions (
    code varchar(12) NOT NULL,
    name varchar(255) NOT NULL,
    level smallint NOT NULL,
    parent_code varchar(12),
    province_code varchar(6),
    province_name varchar(64),
    city_code varchar(6),
    city_name varchar(64),
    district_code varchar(6),
    district_name varchar(64),
    gb28181_code varchar(10),
    CONSTRAINT gb28181_regions_pkey PRIMARY KEY (code)
);
CREATE INDEX IF NOT EXISTS idx_regions_level ON gb28181_regions USING btree (level);
CREATE INDEX IF NOT EXISTS idx_regions_parent ON gb28181_regions USING btree (parent_code);

-- ============================================
-- Table: gb_device_types
-- ============================================
CREATE TABLE IF NOT EXISTS gb_device_types (
    code varchar(10) NOT NULL,
    name varchar(100) NOT NULL,
    name_en varchar(100),
    category varchar(50) NOT NULL,
    description text,
    can_have_children boolean DEFAULT false,
    sort_order integer DEFAULT 0 NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT gb_device_types_pkey PRIMARY KEY (code)
);
CREATE INDEX IF NOT EXISTS idx_gb_device_types_category ON gb_device_types USING btree (category);
CREATE INDEX IF NOT EXISTS idx_gb_device_types_sort ON gb_device_types USING btree (sort_order);

-- ============================================
-- Table: gb_industry_codes
-- ============================================
CREATE TABLE IF NOT EXISTS gb_industry_codes (
    code varchar(10) NOT NULL,
    name varchar(100) NOT NULL,
    name_en varchar(100),
    description text,
    sort_order integer DEFAULT 0 NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT gb_industry_codes_pkey PRIMARY KEY (code)
);
CREATE INDEX IF NOT EXISTS idx_gb_industry_codes_sort ON gb_industry_codes USING btree (sort_order);

-- ============================================
-- Table: gb_network_codes
-- ============================================
CREATE TABLE IF NOT EXISTS gb_network_codes (
    code varchar(10) NOT NULL,
    name varchar(100) NOT NULL,
    name_en varchar(100),
    description text,
    sort_order integer DEFAULT 0 NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT gb_network_codes_pkey PRIMARY KEY (code)
);
CREATE INDEX IF NOT EXISTS idx_gb_network_codes_sort ON gb_network_codes USING btree (sort_order);

-- ============================================
-- Table: map_settings
-- ============================================
CREATE TABLE IF NOT EXISTS map_settings (
    id integer PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    browser_tms_url text NOT NULL DEFAULT '',
    client_tms_url text NOT NULL DEFAULT '',
    wfs_url text NOT NULL DEFAULT '',
    browser_min_zoom integer NOT NULL DEFAULT 0,
    browser_max_zoom integer NOT NULL DEFAULT 19,
    client_min_zoom integer NOT NULL DEFAULT 0,
    client_max_zoom integer NOT NULL DEFAULT 19,
    updated_at timestamptz DEFAULT now() NOT NULL
);

-- ============================================
-- Table: permissions
-- ============================================
CREATE TABLE IF NOT EXISTS permissions (
    id uuid NOT NULL,
    name varchar(255) NOT NULL,
    description text,
    created_at timestamptz DEFAULT now() NOT NULL,
    resource varchar(255),
    action varchar(255),
    CONSTRAINT permissions_pkey PRIMARY KEY (id),
    CONSTRAINT permissions_name_key UNIQUE (name)
);
CREATE INDEX IF NOT EXISTS idx_permissions_name ON permissions USING btree (name);
CREATE UNIQUE INDEX IF NOT EXISTS permissions_name_key ON permissions USING btree (name);

-- ============================================
-- Table: player_window_layouts
-- ============================================
CREATE TABLE IF NOT EXISTS player_window_layouts (
    id integer DEFAULT nextval('player_window_layouts_id_seq'::regclass) NOT NULL,
    name varchar(100) NOT NULL,
    rows integer DEFAULT 2 NOT NULL,
    cols integer DEFAULT 2 NOT NULL,
    layout_json jsonb DEFAULT '[]'::jsonb NOT NULL,
    is_default boolean DEFAULT false NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT player_window_layouts_pkey PRIMARY KEY (id)
);

-- ============================================
-- Table: ptz_control_logs
-- ============================================
CREATE TABLE IF NOT EXISTS ptz_control_logs (
    id bigint DEFAULT nextval('ptz_control_logs_id_seq'::regclass) NOT NULL,
    user_id bigint,
    device_id bigint NOT NULL,
    command varchar(100) NOT NULL,
    speed double precision DEFAULT 1.0 NOT NULL,
    result varchar(50),
    sip_code integer,
    error_message text,
    created_at timestamptz DEFAULT now() NOT NULL,
    call_id varchar(255),
    device_response text,
    CONSTRAINT ptz_control_logs_pkey PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_ptz_logs_created ON ptz_control_logs USING btree (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ptz_logs_device ON ptz_control_logs USING btree (device_id);
CREATE INDEX IF NOT EXISTS idx_ptz_logs_user ON ptz_control_logs USING btree (user_id);

-- ============================================
-- Table: ptz_presets
-- ============================================
CREATE TABLE IF NOT EXISTS ptz_presets (
    id bigint DEFAULT nextval('ptz_presets_id_seq'::regclass) NOT NULL,
    device_id bigint NOT NULL,
    name varchar(255) NOT NULL,
    token varchar(255) NOT NULL,
    position_pan double precision DEFAULT 0 NOT NULL,
    position_tilt double precision DEFAULT 0 NOT NULL,
    position_zoom double precision DEFAULT 0 NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT ptz_presets_pkey PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_ptz_presets_device ON ptz_presets USING btree (device_id);

-- ============================================
-- Table: recording_policies
-- ============================================
CREATE TABLE IF NOT EXISTS recording_policies (
    id bigint DEFAULT nextval('recording_policies_id_seq'::regclass) NOT NULL,
    device_tag varchar(255) NOT NULL,
    channel_tag varchar(255) NOT NULL,
    policy_type varchar(50) NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    schedule_type varchar(50),
    cron_expr text,
    alarm_type varchar(255),
    pre_record_secs integer DEFAULT 0,
    post_record_secs integer DEFAULT 30,
    format varchar(50) DEFAULT 'MP4'::character varying NOT NULL,
    max_duration_secs integer DEFAULT 3600,
    max_file_size_mb integer DEFAULT 512,
    output_path text,
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT recording_policies_pkey PRIMARY KEY (id),
    CONSTRAINT recording_policies_device_tag_channel_tag_policy_type_key UNIQUE (device_tag, channel_tag, policy_type)
);
CREATE INDEX IF NOT EXISTS idx_recording_policies_channel_tag ON recording_policies USING btree (channel_tag);
CREATE INDEX IF NOT EXISTS idx_recording_policies_device_tag ON recording_policies USING btree (device_tag);
CREATE INDEX IF NOT EXISTS idx_recording_policies_enabled ON recording_policies USING btree (enabled);
CREATE INDEX IF NOT EXISTS idx_recording_policies_policy_type ON recording_policies USING btree (policy_type);
CREATE UNIQUE INDEX IF NOT EXISTS recording_policies_device_tag_channel_tag_policy_type_key ON recording_policies USING btree (device_tag, channel_tag, policy_type);

-- ============================================
-- Table: recording_segments
-- ============================================
CREATE TABLE IF NOT EXISTS recording_segments (
    id bigint DEFAULT nextval('recording_segments_id_seq'::regclass) NOT NULL,
    recording_id bigint,
    device_tag varchar(255) NOT NULL,
    channel_tag varchar(255) NOT NULL,
    start_time timestamptz NOT NULL,
    end_time timestamptz NOT NULL,
    duration_secs bigint DEFAULT 0 NOT NULL,
    file_size bigint DEFAULT 0 NOT NULL,
    file_path text NOT NULL,
    server_tag varchar(255),
    segment_type varchar(50) DEFAULT 'Normal'::character varying NOT NULL,
    alarm_id bigint,
    play_url text,
    CONSTRAINT recording_segments_pkey PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_recording_segments_alarm_id ON recording_segments USING btree (alarm_id);
CREATE INDEX IF NOT EXISTS idx_recording_segments_device_channel_time ON recording_segments USING btree (device_tag, channel_tag, start_time, end_time);
CREATE INDEX IF NOT EXISTS idx_recording_segments_recording_id ON recording_segments USING btree (recording_id);

-- ============================================
-- Table: recording_time_windows
-- ============================================
CREATE TABLE IF NOT EXISTS recording_time_windows (
    id bigint DEFAULT nextval('recording_time_windows_id_seq'::regclass) NOT NULL,
    policy_id bigint NOT NULL,
    start_time varchar(10) NOT NULL,
    end_time varchar(10) NOT NULL,
    days integer[] NOT NULL,
    CONSTRAINT recording_time_windows_pkey PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_recording_time_windows_policy_id ON recording_time_windows USING btree (policy_id);

-- ============================================
-- Table: recordings
-- ============================================
CREATE TABLE IF NOT EXISTS recordings (
    id bigint DEFAULT nextval('recordings_id_seq'::regclass) NOT NULL,
    format varchar(50) NOT NULL,
    duration_secs integer DEFAULT 0 NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    server_tag varchar(255),
    state varchar(50),
    output_path text,
    file_size bigint,
    started_at timestamptz,
    stopped_at timestamptz,
    error_message text,
    labels text,
    device_tag varchar(50),
    channel_tag varchar(50),
    filename text,
    max_duration_secs integer DEFAULT 0,
    retry_count integer DEFAULT 0 NOT NULL,
    next_retry_at timestamptz,
    recording_type varchar(32) DEFAULT 'timed'::character varying NOT NULL,
    alarm_id bigint,
    CONSTRAINT recordings_pkey PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_recordings_channel_tag ON recordings USING btree (channel_tag);
CREATE INDEX IF NOT EXISTS idx_recordings_device_tag ON recordings USING btree (device_tag);
CREATE INDEX IF NOT EXISTS idx_recordings_next_retry ON recordings USING btree (state, next_retry_at);
CREATE INDEX IF NOT EXISTS idx_recordings_recording_type ON recordings USING btree (device_tag, channel_tag, recording_type);
CREATE INDEX IF NOT EXISTS idx_recordings_state ON recordings USING btree (state);

-- ============================================
-- Table: role_permissions
-- ============================================
CREATE TABLE IF NOT EXISTS role_permissions (
    role_id uuid NOT NULL,
    permission_id uuid NOT NULL,
    CONSTRAINT role_permissions_pkey PRIMARY KEY (role_id, permission_id)
);

-- ============================================
-- Table: roles
-- ============================================
CREATE TABLE IF NOT EXISTS roles (
    id uuid NOT NULL,
    name varchar(255) NOT NULL,
    description text,
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT roles_pkey PRIMARY KEY (id),
    CONSTRAINT roles_name_key UNIQUE (name)
);
CREATE INDEX IF NOT EXISTS idx_roles_name ON roles USING btree (name);
CREATE UNIQUE INDEX IF NOT EXISTS roles_name_key ON roles USING btree (name);

-- ============================================
-- Table: servers
-- ============================================
CREATE TABLE IF NOT EXISTS servers (
    id bigint DEFAULT nextval('servers_id_seq'::regclass) NOT NULL,
    name varchar(255) NOT NULL,
    url text NOT NULL,
    api_key text NOT NULL,
    server_type varchar(50) NOT NULL,
    weight integer DEFAULT 100 NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    server_tag varchar(255) DEFAULT ''::character varying NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    protocol_ports jsonb DEFAULT '{}'::jsonb NOT NULL,
    CONSTRAINT servers_pkey PRIMARY KEY (id),
    CONSTRAINT servers_server_tag_key UNIQUE (server_tag)
);
CREATE UNIQUE INDEX IF NOT EXISTS servers_server_tag_key ON servers USING btree (server_tag);

-- ============================================
-- Table: sessions
-- ============================================
CREATE TABLE IF NOT EXISTS sessions (
    id bigint DEFAULT nextval('sessions_id_seq'::regclass) NOT NULL,
    session_type varchar(50) NOT NULL,
    user_id bigint,
    created_at timestamptz DEFAULT now() NOT NULL,
    expires_at timestamptz,
    ended_at timestamptz,
    client_ip varchar(45),
    state varchar(50) DEFAULT 'active'::character varying NOT NULL,
    client_type varchar(100),
    tcp_id varchar(64),
    protocol varchar(50),
    updated_at timestamptz DEFAULT now() NOT NULL,
    last_activity timestamptz DEFAULT now() NOT NULL,
    bytes_sent bigint DEFAULT 0 NOT NULL,
    bytes_received bigint DEFAULT 0 NOT NULL,
    device_tag varchar(50),
    channel_tag varchar(50),
    media_server_tag varchar(255),
    CONSTRAINT sessions_pkey PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_sessions_channel_tag ON sessions USING btree (channel_tag);
CREATE INDEX IF NOT EXISTS idx_sessions_device_tag ON sessions USING btree (device_tag);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions USING btree (expires_at);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions USING btree (expires_at);
CREATE INDEX IF NOT EXISTS idx_sessions_last_activity ON sessions USING btree (last_activity);
CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions USING btree (state);
CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions USING btree (state);
CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions USING btree (user_id);

-- ============================================
-- Table: streams
-- ============================================
CREATE TABLE IF NOT EXISTS streams (
    id bigint DEFAULT nextval('streams_id_seq'::regclass) NOT NULL,
    source_url text,
    media_server_name varchar(255),
    state varchar(50) DEFAULT 'active'::character varying NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    stopped_at timestamptz,
    session_id bigint,
    media_server_tag varchar(255),
    updated_at timestamptz DEFAULT now() NOT NULL,
    viewer_count integer DEFAULT 0 NOT NULL,
    token varchar(255),
    bandwidth_in bigint DEFAULT 0,
    bandwidth_out bigint DEFAULT 0,
    last_keepalive_at timestamptz DEFAULT now() NOT NULL,
    auto_recover boolean DEFAULT true NOT NULL,
    retry_count smallint DEFAULT 0 NOT NULL,
    max_retries smallint DEFAULT 20 NOT NULL,
    last_error text,
    app varchar(255) DEFAULT 'live'::character varying NOT NULL,
    device_tag varchar(50),
    channel_tag varchar(50),
    CONSTRAINT streams_pkey PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_streams_device_tag ON streams USING btree (device_tag);
CREATE UNIQUE INDEX IF NOT EXISTS idx_streams_id ON streams USING btree (id);
CREATE INDEX IF NOT EXISTS idx_streams_session_id ON streams USING btree (session_id);
CREATE INDEX IF NOT EXISTS idx_streams_state ON streams USING btree (state);

-- ============================================
-- Table: user_roles
-- ============================================
CREATE TABLE IF NOT EXISTS user_roles (
    user_id uuid NOT NULL,
    role_id uuid NOT NULL,
    CONSTRAINT user_roles_pkey PRIMARY KEY (user_id, role_id)
);

-- ============================================
-- Table: users
-- ============================================
CREATE TABLE IF NOT EXISTS users (
    id uuid NOT NULL,
    username varchar(255) NOT NULL,
    password_hash text NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT users_pkey PRIMARY KEY (id),
    CONSTRAINT users_username_key UNIQUE (username)
);
CREATE INDEX IF NOT EXISTS idx_users_username ON users USING btree (username);
CREATE UNIQUE INDEX IF NOT EXISTS users_username_key ON users USING btree (username);

-- ============================================
-- Foreign Keys
-- ============================================
ALTER TABLE alarms ADD CONSTRAINT alarms_device_id_fkey FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE;
ALTER TABLE recording_segments ADD CONSTRAINT recording_segments_alarm_id_fkey FOREIGN KEY (alarm_id) REFERENCES alarms(id);
ALTER TABLE recording_segments ADD CONSTRAINT recording_segments_recording_id_fkey FOREIGN KEY (recording_id) REFERENCES recordings(id);
ALTER TABLE recording_time_windows ADD CONSTRAINT recording_time_windows_policy_id_fkey FOREIGN KEY (policy_id) REFERENCES recording_policies(id) ON DELETE CASCADE;
ALTER TABLE role_permissions ADD CONSTRAINT role_permissions_permission_id_fkey FOREIGN KEY (permission_id) REFERENCES permissions(id) ON DELETE CASCADE;
ALTER TABLE role_permissions ADD CONSTRAINT role_permissions_role_id_fkey FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE;
ALTER TABLE user_roles ADD CONSTRAINT user_roles_role_id_fkey FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE;
ALTER TABLE user_roles ADD CONSTRAINT user_roles_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

-- ============================================
-- Sequence Ownership
-- ============================================
ALTER SEQUENCE alarms_id_seq OWNED BY alarms.id;
ALTER SEQUENCE channels_id_seq OWNED BY channels.id;
ALTER SEQUENCE device_groups_id_seq OWNED BY device_groups.id;
ALTER SEQUENCE devices_id_seq OWNED BY devices.id;
ALTER SEQUENCE player_window_layouts_id_seq OWNED BY player_window_layouts.id;
ALTER SEQUENCE ptz_control_logs_id_seq OWNED BY ptz_control_logs.id;
ALTER SEQUENCE ptz_presets_id_seq OWNED BY ptz_presets.id;
ALTER SEQUENCE recording_policies_id_seq OWNED BY recording_policies.id;
ALTER SEQUENCE recording_segments_id_seq OWNED BY recording_segments.id;
ALTER SEQUENCE recording_time_windows_id_seq OWNED BY recording_time_windows.id;
ALTER SEQUENCE recordings_id_seq OWNED BY recordings.id;
ALTER SEQUENCE servers_id_seq OWNED BY servers.id;
ALTER SEQUENCE sessions_id_seq OWNED BY sessions.id;
ALTER SEQUENCE streams_id_seq OWNED BY streams.id;

-- ============================================
-- Triggers
-- ============================================
-- 通道坐标写入时自动维护 geom：经度在前，SRID 4326。
-- 任一维度为 NULL 时 geom 置 NULL。
CREATE OR REPLACE FUNCTION channels_sync_geom() RETURNS trigger AS $$
BEGIN
    IF NEW.latitude IS NOT NULL AND NEW.longitude IS NOT NULL THEN
        NEW.geom := ST_SetSRID(ST_MakePoint(NEW.longitude, NEW.latitude), 4326);
    ELSE
        NEW.geom := NULL;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_channels_sync_geom ON channels;
CREATE TRIGGER trg_channels_sync_geom
BEFORE INSERT OR UPDATE OF latitude, longitude ON channels
FOR EACH ROW EXECUTE FUNCTION channels_sync_geom();

-- ============================================
-- Views
-- ============================================
CREATE OR REPLACE VIEW geography_columns AS
 SELECT current_database() AS f_table_catalog,
    n.nspname AS f_table_schema,
    c.relname AS f_table_name,
    a.attname AS f_geography_column,
    postgis_typmod_dims(a.atttypmod) AS coord_dimension,
    postgis_typmod_srid(a.atttypmod) AS srid,
    postgis_typmod_type(a.atttypmod) AS type
   FROM pg_class c,
    pg_attribute a,
    pg_type t,
    pg_namespace n
  WHERE t.typname = 'geography'::name AND a.attisdropped = false AND a.atttypid = t.oid AND a.attrelid = c.oid AND c.relnamespace = n.oid AND (c.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'm'::"char", 'f'::"char", 'p'::"char"])) AND NOT pg_is_other_temp_schema(c.relnamespace) AND has_table_privilege(c.oid, 'SELECT'::text);;

CREATE OR REPLACE VIEW geometry_columns AS
 SELECT current_database()::character varying(256) AS f_table_catalog,
    n.nspname AS f_table_schema,
    c.relname AS f_table_name,
    a.attname AS f_geometry_column,
    COALESCE(postgis_typmod_dims(a.atttypmod), sn.ndims, 2) AS coord_dimension,
    COALESCE(NULLIF(postgis_typmod_srid(a.atttypmod), 0), sr.srid, 0) AS srid,
    replace(replace(COALESCE(NULLIF(upper(postgis_typmod_type(a.atttypmod)), 'GEOMETRY'::text), st.type, 'GEOMETRY'::text), 'ZM'::text, ''::text), 'Z'::text, ''::text)::character varying(30) AS type
   FROM pg_class c
     JOIN pg_attribute a ON a.attrelid = c.oid AND NOT a.attisdropped
     JOIN pg_namespace n ON c.relnamespace = n.oid
     JOIN pg_type t ON a.atttypid = t.oid
     LEFT JOIN ( SELECT s.connamespace,
            s.conrelid,
            s.conkey,
            (regexp_match(s.consrc, 'geometrytype\(\w+\)\s*=\s*''(\w+)'''::text, 'i'::text))[1] AS type
           FROM ( SELECT pg_constraint.connamespace,
                    pg_constraint.conrelid,
                    pg_constraint.conkey,
                    pg_get_constraintdef(pg_constraint.oid) AS consrc
                   FROM pg_constraint) s
          WHERE s.consrc ~* 'geometrytype\(\w+\)\s*=\s*''\w+'''::text) st ON st.connamespace = n.oid AND st.conrelid = c.oid AND (a.attnum = ANY (st.conkey))
     LEFT JOIN ( SELECT s.connamespace,
            s.conrelid,
            s.conkey,
            (regexp_match(s.consrc, 'ndims\(\w+\)\s*=\s*(\d+)'::text, 'i'::text))[1]::integer AS ndims
           FROM ( SELECT pg_constraint.connamespace,
                    pg_constraint.conrelid,
                    pg_constraint.conkey,
                    pg_get_constraintdef(pg_constraint.oid) AS consrc
                   FROM pg_constraint) s
          WHERE s.consrc ~* 'ndims\(\w+\)\s*=\s*\d+'::text) sn ON sn.connamespace = n.oid AND sn.conrelid = c.oid AND (a.attnum = ANY (sn.conkey))
     LEFT JOIN ( SELECT s.connamespace,
            s.conrelid,
            s.conkey,
            (regexp_match(s.consrc, 'srid\(\w+\)\s*=\s*(\d+)'::text, 'i'::text))[1]::integer AS srid
           FROM ( SELECT pg_constraint.connamespace,
                    pg_constraint.conrelid,
                    pg_constraint.conkey,
                    pg_get_constraintdef(pg_constraint.oid) AS consrc
                   FROM pg_constraint) s
          WHERE s.consrc ~* 'srid\(\w+\)\s*=\s*\d+'::text) sr ON sr.connamespace = n.oid AND sr.conrelid = c.oid AND (a.attnum = ANY (sr.conkey))
  WHERE (c.relkind = ANY (ARRAY['r'::"char", 'v'::"char", 'm'::"char", 'f'::"char", 'p'::"char"])) AND NOT c.relname = 'raster_columns'::name AND t.typname = 'geometry'::name AND NOT pg_is_other_temp_schema(c.relnamespace) AND has_table_privilege(c.oid, 'SELECT'::text);;

COMMIT;
