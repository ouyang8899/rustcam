CREATE TABLE IF NOT EXISTS gb28181_alarms (
    id BIGSERIAL PRIMARY KEY,
    device_id BIGINT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    device_tag VARCHAR(64) NOT NULL,
    alarm_type VARCHAR(64) NOT NULL DEFAULT 'Alarm',
    alarm_time TIMESTAMP NOT NULL,
    alarm_method INT DEFAULT 1,
    alarm_priority INT DEFAULT 0,
    description TEXT,
    processed BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_alarms_device_time ON gb28181_alarms(device_id, alarm_time DESC);
CREATE INDEX IF NOT EXISTS idx_alarms_processed ON gb28181_alarms(processed) WHERE NOT processed;