#!/usr/bin/env python3
"""
Migration script for the map layer configuration.

Replaces the flat 2D-only columns of map_settings with three jsonb columns:
  - browser_layers  jsonb array of layers for the web map (dim 2d and 3d)
  - client_layers   jsonb array of layers for the desktop client map
  - wfs             jsonb object { url, srs }

The old columns browser_tms_url / client_tms_url / wfs_url are backfilled into
the new structure and then dropped, so that there is a single source of truth.

The end-level zoom columns (browser_min_zoom / browser_max_zoom /
client_min_zoom / client_max_zoom) are also dropped: every renderer now derives
its zoom range from the layer it displays, so keeping them would allow a view
zoom range that contradicts the imagery.

Idempotent: safe to run repeatedly.

Usage: python migrate_map_layers.py
"""

import json
import os
import re
import sys
from urllib.parse import urlparse

import psycopg2

CONFIG_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "config.toml")


def parse_config():
    """Read the database URL from config.toml"""
    with open(CONFIG_PATH, "r", encoding="utf-8") as f:
        content = f.read()
    match = re.search(r'url\s*=\s*["\']([^"\']+)["\']', content)
    if not match:
        raise ValueError("Could not find database URL in config.toml")
    return match.group(1)


def get_connection(db_url):
    """Create database connection (the URL may carry extra query params)"""
    parsed = urlparse(db_url)
    return psycopg2.connect(
        host=parsed.hostname or "localhost",
        port=parsed.port or 5432,
        database=parsed.path.lstrip("/") or "rustcam",
        user=parsed.username or "postgres",
        password=parsed.password or "",
    )


def build_layers(raw, dim, min_zoom, max_zoom):
    """Split a legacy multi-line tile url into one layer per line."""
    layers = []
    for index, line in enumerate((raw or "").split("\n")):
        url = line.strip()
        if not url:
            continue
        layers.append({
            "id": "layer-%d" % index,
            "name": "",
            "dim": dim,
            "url": url,
            "srs": "EPSG:3857",
            "protocol": detect_protocol(url),
            "format": detect_format(url),
            "tile_size": 256,
            "min_zoom": min_zoom,
            "max_zoom": max_zoom,
            "enabled": True,
        })
    return layers


def detect_protocol(url):
    """Guess the tile protocol from the url structure."""
    lower = url.lower()
    if "{quadkey}" in lower:
        return "quadkey"
    if "tilematrix" in lower and "tilerow" in lower and "tilecol" in lower:
        return "wmts"
    if "{-y}" in lower:
        return "tms"
    return "xyz"


def detect_format(url):
    """Guess the image format from the url extension."""
    path = url.split("?")[0].lower()
    if path.endswith(".png"):
        return "png"
    if path.endswith(".jpg") or path.endswith(".jpeg"):
        return "jpeg"
    if path.endswith(".webp"):
        return "webp"
    return ""


def main():
    conn = get_connection(parse_config())
    conn.autocommit = True
    cur = conn.cursor()

    statements = [
        "ALTER TABLE map_settings ADD COLUMN IF NOT EXISTS browser_layers jsonb NOT NULL DEFAULT '[]'::jsonb",
        "ALTER TABLE map_settings ADD COLUMN IF NOT EXISTS client_layers jsonb NOT NULL DEFAULT '[]'::jsonb",
        "ALTER TABLE map_settings ADD COLUMN IF NOT EXISTS wfs jsonb NOT NULL DEFAULT '{}'::jsonb",
    ]
    for stmt in statements:
        try:
            cur.execute(stmt)
            print("OK   %s" % stmt)
        except Exception as exc:  # noqa: BLE001
            print("FAIL %s -> %s" % (stmt, exc))
            conn.rollback()
            return 1

    # Backfill from the legacy columns (only when they still exist).
    cur.execute(
        "select column_name from information_schema.columns "
        "where table_name = 'map_settings' and column_name in "
        "('browser_tms_url', 'client_tms_url', 'wfs_url')"
    )
    legacy = {row[0] for row in cur.fetchall()}

    try:
        if "browser_tms_url" in legacy:
            cur.execute(
                "select column_name from information_schema.columns "
                "where table_name = 'map_settings'"
            )
            existing = {row[0] for row in cur.fetchall()}

            def zoom_col(name, default):
                return name if name in existing else str(default)

            cur.execute(
                "select browser_tms_url, client_tms_url, wfs_url, "
                + ", ".join(
                    [
                        zoom_col("browser_min_zoom", 0),
                        zoom_col("browser_max_zoom", 19),
                        zoom_col("client_min_zoom", 0),
                        zoom_col("client_max_zoom", 19),
                    ]
                )
                + " from map_settings where id = 1"
            )
            row = cur.fetchone()
            if row:
                browser_url, client_url, wfs_url, bmin, bmax, cmin, cmax = row
                browser_layers = build_layers(browser_url, "2d", bmin, bmax)
                client_layers = build_layers(client_url, "2d", cmin, cmax)
                wfs = {"url": (wfs_url or "").strip(), "srs": "EPSG:4326"}
                cur.execute(
                    "update map_settings set browser_layers = %s::jsonb, "
                    "client_layers = %s::jsonb, wfs = %s::jsonb where id = 1",
                    (
                        json.dumps(browser_layers),
                        json.dumps(client_layers),
                        json.dumps(wfs),
                    ),
                )
                print("OK   backfilled browser_layers/client_layers/wfs")
        else:
            print("SKIP legacy columns already dropped, nothing to backfill")
    except Exception as exc:  # noqa: BLE001
        print("FAIL backfill -> %s" % exc)
        conn.rollback()
        return 1

    for stmt in [
        "ALTER TABLE map_settings DROP COLUMN IF EXISTS browser_tms_url",
        "ALTER TABLE map_settings DROP COLUMN IF EXISTS client_tms_url",
        "ALTER TABLE map_settings DROP COLUMN IF EXISTS wfs_url",
        "ALTER TABLE map_settings DROP COLUMN IF EXISTS browser_min_zoom",
        "ALTER TABLE map_settings DROP COLUMN IF EXISTS browser_max_zoom",
        "ALTER TABLE map_settings DROP COLUMN IF EXISTS client_min_zoom",
        "ALTER TABLE map_settings DROP COLUMN IF EXISTS client_max_zoom",
    ]:
        try:
            cur.execute(stmt)
            print("OK   %s" % stmt)
        except Exception as exc:  # noqa: BLE001
            print("FAIL %s -> %s" % (stmt, exc))
            conn.rollback()
            return 1

    cur.execute(
        "select column_name, data_type from information_schema.columns "
        "where table_name = 'map_settings' order by ordinal_position"
    )
    for row in cur.fetchall():
        print(row)

    conn.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
