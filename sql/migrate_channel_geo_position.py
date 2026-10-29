"""把地理坐标从 devices 迁移到 channels。

devices.latitude / longitude / geom 三列直接删除，存量坐标是
fallback_map_position() 算出来的假散点，无迁移价值。
channels 侧新增三列，并由 channels_sync_geom() 触发器自动填充 geom。

幂等：可重复执行。

用法：python sql/migrate_channel_geo_position.py
"""
import os
import re
import sys

import psycopg2

sys.stdout.reconfigure(encoding="utf-8")


def parse_config():
    """从 config.toml 的 [database] 段取 url。

    整个文件里有多个 `url = `（database / redis），
    所以必须先定位 [database] 段，不能全局取第一个。
    """
    config_path = os.path.join(os.path.dirname(__file__), "..", "config.toml")
    with open(config_path, "r", encoding="utf-8") as f:
        content = f.read()

    section = re.search(r"^\[database\]\s*$(.*?)(?=^\[|\Z)", content, re.MULTILINE | re.DOTALL)
    if not section:
        raise RuntimeError("[database] section not found in config.toml")

    match = re.search(r'^url\s*=\s*"([^"]+)"', section.group(1), re.MULTILINE)
    if not match:
        raise RuntimeError("database url not found in config.toml")
    return match.group(1)


def get_connection(db_url):
    # config.toml 里的 url 带 `?options=-c%20client_encoding=UTF8`，
    # psycopg2 不认这个参数，剥掉即可。
    url = db_url.split("?")[0]
    return psycopg2.connect(url)


SQL_STATEMENTS = [
    """
    CREATE EXTENSION IF NOT EXISTS postgis WITH SCHEMA public
    """,
    """
    ALTER TABLE channels ADD COLUMN IF NOT EXISTS latitude double precision
    """,
    """
    ALTER TABLE channels ADD COLUMN IF NOT EXISTS longitude double precision
    """,
    """
    ALTER TABLE channels ADD COLUMN IF NOT EXISTS geom geometry(Point,4326)
    """,
    """
    CREATE INDEX IF NOT EXISTS idx_channels_geom ON channels USING GIST (geom)
    """,
    """
    CREATE OR REPLACE FUNCTION channels_sync_geom() RETURNS trigger AS $$
    BEGIN
        IF NEW.latitude IS NOT NULL AND NEW.longitude IS NOT NULL THEN
            NEW.geom := ST_SetSRID(ST_MakePoint(NEW.longitude, NEW.latitude), 4326);
        ELSE
            NEW.geom := NULL;
        END IF;
        RETURN NEW;
    END;
    $$ LANGUAGE plpgsql
    """,
    """
    DROP TRIGGER IF EXISTS trg_channels_sync_geom ON channels
    """,
    """
    CREATE TRIGGER trg_channels_sync_geom
    BEFORE INSERT OR UPDATE OF latitude, longitude ON channels
    FOR EACH ROW EXECUTE FUNCTION channels_sync_geom()
    """,
    """
    DROP TRIGGER IF EXISTS trg_devices_sync_geom ON devices
    """,
    """
    DROP FUNCTION IF EXISTS devices_sync_geom()
    """,
    """
    ALTER TABLE devices DROP COLUMN IF EXISTS geom
    """,
    """
    ALTER TABLE devices DROP COLUMN IF EXISTS latitude
    """,
    """
    ALTER TABLE devices DROP COLUMN IF EXISTS longitude
    """,
]


def main():
    db_url = parse_config()
    print(f"db: {db_url.split('?')[0]}")
    conn = get_connection(db_url)
    conn.autocommit = True
    cur = conn.cursor()

    # 破坏性操作前先留一份 devices 侧坐标备份，验证通过后由 --drop-backup 清掉。
    cur.execute("DROP TABLE IF EXISTS devices_geo_backup")
    cur.execute(
        """
        CREATE TABLE devices_geo_backup AS
        SELECT id, device_tag, name, latitude, longitude, geom FROM devices
        """
    )
    print("[backup] devices_geo_backup 已创建")

    for sql in SQL_STATEMENTS:
        cur.execute(sql)
        print(f"[ok] {sql.strip().splitlines()[0][:80]}")

    print("\n=== 校验 ===")
    cur.execute(
        """
        SELECT column_name FROM information_schema.columns
        WHERE table_name = 'channels' AND column_name IN ('latitude','longitude','geom')
        ORDER BY column_name
        """
    )
    print("channels 地理列:", [r[0] for r in cur.fetchall()])

    cur.execute(
        """
        SELECT column_name FROM information_schema.columns
        WHERE table_name = 'devices' AND column_name IN ('latitude','longitude','geom')
        """
    )
    print("devices 残留地理列:", [r[0] for r in cur.fetchall()])

    cur.execute(
        """
        SELECT tgname, pg_get_triggerdef(oid) FROM pg_trigger
        WHERE tgrelid = 'channels'::regclass AND NOT tgisinternal
        """
    )
    for row in cur.fetchall():
        print("channels 触发器:", row)

    cur.execute(
        """
        SELECT count(*) FROM pg_trigger
        WHERE tgrelid = 'devices'::regclass AND NOT tgisinternal
        """
    )
    print("devices 残留触发器数:", cur.fetchone()[0])

    cur.execute(
        "SELECT count(*) FROM channels WHERE geom IS NOT NULL AND (latitude IS NULL OR longitude IS NULL)"
    )
    print("坐标与 geom 不一致的通道数:", cur.fetchone()[0])

    conn.close()


if __name__ == "__main__":
    main()
