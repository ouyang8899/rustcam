CREATE TABLE IF NOT EXISTS video_tour_schemes (
    id bigserial PRIMARY KEY,
    name varchar(100) NOT NULL,
    version integer NOT NULL DEFAULT 1,
    layout_id integer NOT NULL,
    device_channels jsonb NOT NULL DEFAULT '[]'::jsonb,
    locked_set jsonb NOT NULL DEFAULT '[]'::jsonb,
    dwell_ms integer NOT NULL DEFAULT 10000,
    batch_timeout_ms integer NOT NULL DEFAULT 5000,
    switch_gap_ms integer NOT NULL DEFAULT 1000,
    cycle_gap_ms integer NOT NULL DEFAULT 0,
    max_cycles integer,
    run_window jsonb,
    safety_max_duration_ms bigint,
    backfill_limit integer NOT NULL DEFAULT 2,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_video_tour_schemes_name ON video_tour_schemes (name);
