CREATE EXTENSION IF NOT EXISTS postgis;

CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    mode TEXT NOT NULL CHECK (mode IN ('live', 'historical', 'synthetic')),
    epoch BIGINT NOT NULL DEFAULT 0 CHECK (epoch >= 0),
    state JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS evidence (
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    origin_group TEXT NOT NULL,
    event_at TIMESTAMPTZ,
    captured_at TIMESTAMPTZ NOT NULL,
    available_at TIMESTAMPTZ NOT NULL,
    position GEOMETRY(Point, 4326),
    record JSONB NOT NULL,
    PRIMARY KEY(run_id, id)
);
CREATE INDEX evidence_available_idx ON evidence(run_id, available_at);
CREATE INDEX evidence_position_idx ON evidence USING gist(position);

CREATE TABLE IF NOT EXISTS source_runs (
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    batch JSONB NOT NULL,
    PRIMARY KEY(run_id, source_id, captured_at)
);

CREATE TABLE IF NOT EXISTS webhook_receipts (
    id TEXT PRIMARY KEY,
    body_hash TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
