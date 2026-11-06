#!/usr/bin/env python3
"""为 drone_device 增加机载链路身份字段（drone.md §9.1 / §12.4）。

幂等：可重复执行。存量库用本脚本补列；新建库由 drone-service 的 migrations.sql 建全。

新增列：
  unique_id                登记的无人机唯一标识（AUTOPILOT_VERSION.uid）
  mavlink_addr             该机的 MAVLink 端点（隧道内地址）
  signing_secret           该机的 MAVLink 2 签名密钥（密文存储）
  signing_allow_unsigned   是否放行未签名帧（含 MAVLink 1 帧）
  mavlink_version          MAVLink 协议版本（1 / 2）
  mavlink_sysid            该机的 system id
  mavlink_compid           该机的 component id

用法：python sql/migrate_drone_link_identity.py
"""

import os
import sys

import psycopg2

COLUMNS = [
    ("unique_id", "text"),
    ("mavlink_addr", "text"),
    ("signing_secret", "text"),
    ("signing_allow_unsigned", "boolean NOT NULL DEFAULT true"),
    ("mavlink_version", "smallint NOT NULL DEFAULT 2"),
    ("mavlink_sysid", "smallint NOT NULL DEFAULT 1"),
    ("mavlink_compid", "smallint NOT NULL DEFAULT 1"),
]

INDEXES = [
    "CREATE INDEX IF NOT EXISTS idx_drone_device_adapter_id ON drone_device (adapter_id)",
]


def main() -> int:
    url = os.environ.get(
        "DATABASE_URL", "postgres://postgres:123456@192.168.1.7:5435/rustcam"
    )
    conn = psycopg2.connect(url)
    try:
        with conn.cursor() as cur:
            for name, ddl in COLUMNS:
                cur.execute(
                    f"ALTER TABLE drone_device ADD COLUMN IF NOT EXISTS {name} {ddl}"
                )
            for stmt in INDEXES:
                cur.execute(stmt)
        conn.commit()
    finally:
        conn.close()
    print("drone_device 机载链路身份字段已就绪")
    return 0


if __name__ == "__main__":
    sys.exit(main())
