-- drone-service 自带的幂等 DDL；启动时由 DroneDb::run_migrations() 逐条执行。
-- 主服务 sql/schema.sql 与 DbRepository::ensure_schema 不感知这些表。

CREATE TABLE IF NOT EXISTS drone_device (
  drone_tag     text PRIMARY KEY,
  protocol      text NOT NULL,
  adapter_id    text,
  online        boolean NOT NULL DEFAULT false,
  last_seen     timestamptz,
  device_tag    text,
  channel_tag   text,
  video_stream  text,
  unique_id     text,
  mavlink_addr  text,
  signing_secret text,
  signing_allow_unsigned boolean NOT NULL DEFAULT true,
  mavlink_version smallint NOT NULL DEFAULT 2,
  mavlink_sysid   smallint NOT NULL DEFAULT 1,
  mavlink_compid  smallint NOT NULL DEFAULT 1,
  meta          jsonb,
  created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_drone_device_device_tag ON drone_device (device_tag);
CREATE INDEX IF NOT EXISTS idx_drone_device_channel_tag ON drone_device (channel_tag);
CREATE INDEX IF NOT EXISTS idx_drone_device_adapter_id ON drone_device (adapter_id);

-- 链路绑定：同一 adapter 内 sysid 唯一；adapter_id 为空或 sysid 未指定的行不参与约束。
CREATE UNIQUE INDEX IF NOT EXISTS uq_drone_device_adapter_sysid
    ON drone_device (adapter_id, mavlink_sysid)
    WHERE adapter_id <> '' AND mavlink_sysid > 0;

CREATE TABLE IF NOT EXISTS drone_control_audit (
  id           bigserial PRIMARY KEY,
  ts           timestamptz NOT NULL DEFAULT now(),
  drone_tag    text NOT NULL,
  control_type text NOT NULL,
  action       text NOT NULL,
  holder       text,
  priority     int,
  result       text NOT NULL,
  operator     text
);
CREATE INDEX IF NOT EXISTS idx_drone_control_audit_device_ts ON drone_control_audit (drone_tag, ts DESC);

CREATE TABLE IF NOT EXISTS drone_command_audit (
  id           bigserial PRIMARY KEY,
  ts           timestamptz NOT NULL DEFAULT now(),
  drone_tag    text NOT NULL,
  cmd_id       text NOT NULL UNIQUE,
  type         text NOT NULL,
  params       jsonb,
  status       text NOT NULL,
  operator     text,
  finished_at  timestamptz
);
CREATE INDEX IF NOT EXISTS idx_drone_command_audit_device_ts ON drone_command_audit (drone_tag, ts DESC);

CREATE TABLE IF NOT EXISTS drone_geofence (
  id          bigserial PRIMARY KEY,
  name        text NOT NULL,
  type        text NOT NULL,
  min_alt     double precision,
  max_alt     double precision,
  geojson     text NOT NULL,
  enabled     boolean NOT NULL DEFAULT true,
  version     int NOT NULL DEFAULT 1,
  created_by  text,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_drone_geofence_enabled ON drone_geofence (enabled);

-- 航线：waypoints 存 JSONB，drone_tags 存适用无人机列表（多对多）。
CREATE TABLE IF NOT EXISTS drone_route (
  id          text PRIMARY KEY,
  name        text NOT NULL,
  drone_tags  jsonb NOT NULL DEFAULT '[]'::jsonb,
  waypoints   jsonb NOT NULL DEFAULT '[]'::jsonb,
  enabled     boolean NOT NULL DEFAULT true,
  end_action  text NOT NULL DEFAULT 'hold',
  updated_ms  bigint NOT NULL DEFAULT 0,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_drone_route_enabled ON drone_route (enabled);

-- 围栏：drone_tags 存适用无人机列表；几何/高度字段与运行时模型一致。
CREATE TABLE IF NOT EXISTS drone_fence (
  id          text PRIMARY KEY,
  name        text NOT NULL,
  type        text NOT NULL,
  shape       text NOT NULL DEFAULT 'polygon',
  geojson     text NOT NULL DEFAULT '',
  center_lat  double precision NOT NULL DEFAULT 0,
  center_lon  double precision NOT NULL DEFAULT 0,
  radius_m    double precision NOT NULL DEFAULT 0,
  min_alt     double precision NOT NULL DEFAULT 0,
  max_alt     double precision NOT NULL DEFAULT 0,
  drone_tags  jsonb NOT NULL DEFAULT '[]'::jsonb,
  enabled     boolean NOT NULL DEFAULT true,
  version     int NOT NULL DEFAULT 1,
  updated_ms  bigint NOT NULL DEFAULT 0,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_drone_fence_enabled ON drone_fence (enabled);
