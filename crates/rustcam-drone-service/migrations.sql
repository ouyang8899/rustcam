-- drone-service 自带的幂等 DDL；启动时由 DroneDb::run_migrations() 逐条执行。
-- 主服务 sql/schema.sql 与 DbRepository::ensure_schema 不感知这些表。

-- 遥测归档用 TimescaleDB 超表（10Hz 连续采样是唯一的真时序数据）。
-- 扩展需随 PG 预加载（shared_preload_libraries = 'timescaledb'），随包安装。
CREATE EXTENSION IF NOT EXISTS timescaledb;

CREATE TABLE IF NOT EXISTS drone_device (
  drone_tag     text PRIMARY KEY,
  protocol      text NOT NULL,
  adapter_id    text,
  online        boolean NOT NULL DEFAULT false,
  last_seen     timestamptz,
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

-- 航线：waypoints 存 JSONB；适用无人机走 drone_route_device 关联表（多对多）。
CREATE TABLE IF NOT EXISTS drone_route (
  id          text PRIMARY KEY,
  name        text NOT NULL,
  waypoints   jsonb NOT NULL DEFAULT '[]'::jsonb,
  enabled     boolean NOT NULL DEFAULT true,
  end_action  text NOT NULL DEFAULT 'hold',
  updated_ms  bigint NOT NULL DEFAULT 0,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_drone_route_enabled ON drone_route (enabled);

-- 航线 ↔ 无人机（多对多）；drone_tag 不外键到 drone_device：航线允许先于机体建档。
CREATE TABLE IF NOT EXISTS drone_route_device (
  route_id   text NOT NULL REFERENCES drone_route (id) ON DELETE CASCADE,
  drone_tag  text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (route_id, drone_tag)
);
CREATE INDEX IF NOT EXISTS idx_drone_route_device_drone_tag ON drone_route_device (drone_tag);

-- 围栏：几何/高度字段与运行时模型一致；适用无人机走 drone_fence_device 关联表。
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
  enabled     boolean NOT NULL DEFAULT true,
  version     int NOT NULL DEFAULT 1,
  updated_ms  bigint NOT NULL DEFAULT 0,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_drone_fence_enabled ON drone_fence (enabled);

-- 围栏 ↔ 无人机（多对多）。
CREATE TABLE IF NOT EXISTS drone_fence_device (
  fence_id   text NOT NULL REFERENCES drone_fence (id) ON DELETE CASCADE,
  drone_tag  text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (fence_id, drone_tag)
);
CREATE INDEX IF NOT EXISTS idx_drone_fence_device_drone_tag ON drone_fence_device (drone_tag);

-- 废弃表：drone_geofence 是 fence 改名遗留的废表，代码零读写，直接清理。
DROP TABLE IF EXISTS drone_geofence;

-- ---------------------------------------------------------------------------
-- 遥测归档（时序）
-- ---------------------------------------------------------------------------
-- 只存逐帧变化的量：unique_id / autopilot / home_* 每机恒定，档案表里已有，
-- 逐帧重复存纯属浪费空间。
CREATE TABLE IF NOT EXISTS drone_telemetry (
  drone_tag    text NOT NULL,
  ts           timestamptz NOT NULL,
  lat          double precision NOT NULL,
  lon          double precision NOT NULL,
  alt_amsl     double precision NOT NULL,
  rel_alt      double precision NOT NULL,
  roll         double precision NOT NULL,
  pitch        double precision NOT NULL,
  yaw          double precision NOT NULL,
  vx           double precision NOT NULL,
  vy           double precision NOT NULL,
  vz           double precision NOT NULL,
  qx           double precision NOT NULL,
  qy           double precision NOT NULL,
  qz           double precision NOT NULL,
  qw           double precision NOT NULL,
  battery_pct  double precision NOT NULL,
  voltage      double precision NOT NULL,
  link_quality integer NOT NULL,
  seq          integer NOT NULL,
  satellites   integer NOT NULL,
  gps_fix      integer NOT NULL,
  hdop         double precision NOT NULL,
  armed        boolean NOT NULL,
  mode         text NOT NULL,
  PRIMARY KEY (drone_tag, ts)
);
SELECT create_hypertable(
  'drone_telemetry', 'ts',
  chunk_time_interval => INTERVAL '1 day',
  if_not_exists => TRUE
);

-- 3 天后的数据压缩（按 drone_tag 分段，ts 倒序），保留期 7 天由后台策略清理。
ALTER TABLE drone_telemetry SET (
  timescaledb.compress,
  timescaledb.compress_segmentby = 'drone_tag',
  timescaledb.compress_orderby = 'ts DESC'
);
SELECT add_compression_policy('drone_telemetry', INTERVAL '3 days', if_not_exists => TRUE);
SELECT add_retention_policy('drone_telemetry', INTERVAL '7 days', if_not_exists => TRUE);

-- ---------------------------------------------------------------------------
-- 告警归档（事件，低频，普通表不做分区）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS drone_alert (
  id         bigserial PRIMARY KEY,
  ts         timestamptz NOT NULL,
  drone_tag  text NOT NULL,
  rule       text NOT NULL,
  level      text NOT NULL,
  message    text NOT NULL,
  lat        double precision,
  lon        double precision,
  alt        double precision
);
CREATE INDEX IF NOT EXISTS idx_drone_alert_drone_ts ON drone_alert (drone_tag, ts DESC);
CREATE INDEX IF NOT EXISTS idx_drone_alert_ts ON drone_alert (ts DESC);
