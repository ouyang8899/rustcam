-- drone-service 自带的幂等 DDL；启动时由 DroneDb::run_migrations() 逐条执行。
-- 主服务 sql/schema.sql 与 DbRepository::ensure_schema 不感知这些表。

-- 遥测归档用 TimescaleDB 超表（10Hz 连续采样是唯一的真时序数据）。
-- 扩展需随 PG 预加载（shared_preload_libraries = 'timescaledb'），随包安装。
CREATE EXTENSION IF NOT EXISTS timescaledb;

-- 机载链路实体：adapter 自身的配置与运行状态。
-- 飞控链路身份（协议族 / 端口 / 飞控系统号）按无人机登记在 drone_device 上。
CREATE TABLE IF NOT EXISTS drone_adapter (
  adapter_id              text PRIMARY KEY,
  name                    text NOT NULL DEFAULT '',
  protocol                text NOT NULL DEFAULT 'mavlink',
  bind_addr               text NOT NULL DEFAULT '0.0.0.0',
  -- 可用端口段：端口数 × 255 架（每端口按 sysid 区分）。
  port_start              smallint NOT NULL DEFAULT 14540,
  port_end                smallint NOT NULL DEFAULT 14560,
  autopilot               text NOT NULL DEFAULT 'auto',
  signing_secret          text,
  signing_allow_unsigned  boolean NOT NULL DEFAULT true,
  reconnect_min_secs      int NOT NULL DEFAULT 1,
  reconnect_max_secs      int NOT NULL DEFAULT 30,
  online                  boolean NOT NULL DEFAULT false,
  -- 启停状态：false = 已停用（软删除），配置行与绑定关系保留以便回滚。
  enabled                 boolean NOT NULL DEFAULT true,
  last_seen               timestamptz,
  created_at              timestamptz NOT NULL DEFAULT now(),
  updated_at              timestamptz NOT NULL DEFAULT now()
);

-- 旧库补列：软删除能力是后加的，历史库没有 enabled 列。
ALTER TABLE drone_adapter ADD COLUMN IF NOT EXISTS enabled boolean NOT NULL DEFAULT true;

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
  -- 链路身份的协议中立拆解（由 mavlink_addr / mavlink_sysid 归一化而来）：
  -- 传输族（tcp / udp / serial，不含 in/out 方向）+ 端点（端口或串口）+ 飞控系统号。
  link_family   text,
  link_endpoint text,
  fc_sysid      smallint,
  meta          jsonb,
  created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_drone_device_adapter_id ON drone_device (adapter_id);

-- 存量库补列（新建库时上面的 CREATE TABLE 已含这些列）。
ALTER TABLE drone_device ADD COLUMN IF NOT EXISTS link_family   text;
ALTER TABLE drone_device ADD COLUMN IF NOT EXISTS link_endpoint text;
ALTER TABLE drone_device ADD COLUMN IF NOT EXISTS fc_sysid      smallint;

-- 回填：把既有的 mavlink_addr / mavlink_sysid 归一化成协议中立的链路身份。
UPDATE drone_device
SET link_family = CASE
        WHEN mavlink_addr LIKE 'tcp%' THEN 'tcp'
        WHEN mavlink_addr LIKE 'serial:%' THEN 'serial'
        ELSE 'udp'
    END,
    link_endpoint = CASE
        WHEN mavlink_addr LIKE 'serial:%' THEN split_part(mavlink_addr, ':', 2)
        WHEN mavlink_addr LIKE '%:%:%' THEN split_part(mavlink_addr, ':', 3)
        ELSE NULL
    END,
    fc_sysid = mavlink_sysid
WHERE (link_family IS NULL OR link_endpoint IS NULL OR fc_sysid IS NULL)
  AND mavlink_addr IS NOT NULL AND mavlink_addr <> '';

-- 链路绑定：同一 adapter 内「协议族 + 端点 + 飞控系统号」三者组合唯一。
-- 同一端口可承载多架机（按 sysid 区分），因此端点与 sysid 必须同时参与约束。
DROP INDEX IF EXISTS uq_drone_device_adapter_sysid;
CREATE UNIQUE INDEX IF NOT EXISTS uq_drone_device_link_identity
    ON drone_device (adapter_id, link_family, link_endpoint, fc_sysid)
    WHERE adapter_id IS NOT NULL AND adapter_id <> ''
      AND link_family IS NOT NULL AND link_endpoint IS NOT NULL
      AND fc_sysid IS NOT NULL AND fc_sysid > 0;

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
