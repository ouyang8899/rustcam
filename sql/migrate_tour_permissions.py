#!/usr/bin/env python3
"""
Migration script for the video tour permission points.

Inserts the video tour permission points into the `permissions` table and grants
them to the matching roles through `role_permissions`. Roles that do not exist
are skipped.

Idempotent: safe to run repeatedly.

Usage: python migrate_tour_permissions.py
"""

import os
import re
import sys
from urllib.parse import urlparse

import psycopg2

CONFIG_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "config.toml")

# (权限名, 描述) —— 与视频轮询插件保持一致
PERMISSIONS = [
    ("tour:view", "查看视频轮询方案"),
    ("tour:manage", "管理视频轮询方案"),
]

GRANTS = {"Admin": "tour:manage", "Operator": "tour:manage", "Viewer": "tour:view"}


def parse_config():
    with open(CONFIG_PATH, "r", encoding="utf-8") as f:
        content = f.read()
    match = re.search(r'url\s*=\s*["\']([^"\']+)["\']', content)
    if not match:
        raise ValueError("Could not find database URL in config.toml")
    return match.group(1)


def get_connection(db_url):
    parsed = urlparse(db_url)
    return psycopg2.connect(
        host=parsed.hostname or "localhost",
        port=parsed.port or 5432,
        database=parsed.path.lstrip("/") or "rustcam",
        user=parsed.username or "postgres",
        password=parsed.password or "",
    )


def main():
    conn = get_connection(parse_config())
    conn.autocommit = True
    cur = conn.cursor()

    for name, description in PERMISSIONS:
        cur.execute(
            "insert into permissions (id, name, description) "
            "values (gen_random_uuid(), %s, %s) "
            "on conflict (name) do update set description = excluded.description",
            (name, description),
        )
        print("OK   permission", name)

    for role_name, permission in GRANTS.items():
        cur.execute(
            "insert into role_permissions (role_id, permission_id) "
            "select r.id, p.id from roles r join permissions p on p.name = %s "
            "where r.name = %s on conflict do nothing",
            (permission, role_name),
        )
        print("OK   grant", permission, "to role", role_name)

    cur.execute(
        "select r.name, p.name from role_permissions rp "
        "join roles r on r.id = rp.role_id "
        "join permissions p on p.id = rp.permission_id "
        "where p.name like 'tour:%' order by r.name, p.name"
    )
    for row in cur.fetchall():
        print(row)

    conn.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
