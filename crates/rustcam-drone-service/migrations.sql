-- drone-service 自带的幂等 DDL；启动时由 DroneDb::run_migrations() 逐条执行。
-- 主服务 sql/schema.sql 与 DbRepository::ensure_schema 不感知这些表。

CREATE TABLE IF NOT EXISTS drone_device (
  device_id     text PRIMARY KEY,
  protocol      text NOT NULL,
  adapter_id    text,
  online        boolean NOT NULL DEFAULT false,
  last_seen     timestamptz,
  device_tag    text,
  channel_tag   text,
  video_stream  text,
  meta          jsonb,
  created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_drone_device_device_tag ON drone_device (device_tag);
CREATE INDEX IF NOT EXISTS idx_drone_device_channel_tag ON drone_device (channel_tag);

CREATE TABLE IF NOT EXISTS drone_control_audit (
  id           bigserial PRIMARY KEY,
  ts           timestamptz NOT NULL DEFAULT now(),
  device_id    text NOT NULL,
  control_type text NOT NULL,
  action       text NOT NULL,
  holder       text,
  priority     int,
  result       text NOT NULL,
  operator     text
);
CREATE INDEX IF NOT EXISTS idx_drone_control_audit_device_ts ON drone_control_audit (device_id, ts DESC);

CREATE TABLE IF NOT EXISTS drone_command_audit (
  id           bigserial PRIMARY KEY,
  ts           timestamptz NOT NULL DEFAULT now(),
  device_id    text NOT NULL,
  cmd_id       text NOT NULL,
  command_type text NOT NULL,
  params       jsonb,
  result       text NOT NULL,
  operator     text
);
CREATE INDEX IF NOT EXISTS idx_drone_command_audit_device_ts ON drone_command_audit (device_id, ts DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_drone_command_audit_cmd_id ON drone_command_audit (cmd_id);

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
