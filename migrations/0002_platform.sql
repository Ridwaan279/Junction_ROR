CREATE EXTENSION IF NOT EXISTS postgis;

CREATE TABLE platform_state (
    singleton boolean PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    snapshot jsonb NOT NULL
);
CREATE TABLE platform_runs (
    id uuid PRIMARY KEY,
    mode text NOT NULL CHECK (mode IN ('live','historical','synthetic')),
    epoch bigint NOT NULL CHECK (epoch >= 0),
    state jsonb NOT NULL
);
CREATE TABLE platform_evidence (
    run_id uuid NOT NULL REFERENCES platform_runs(id),
    id uuid NOT NULL,
    source_id text NOT NULL,
    external_id text NOT NULL,
    available_at timestamptz NOT NULL,
    geom geography(Point,4326),
    record jsonb NOT NULL,
    PRIMARY KEY (run_id,id)
);
CREATE INDEX evidence_available ON platform_evidence(run_id,available_at);
CREATE INDEX evidence_geometry ON platform_evidence USING gist(geom);
CREATE TABLE platform_incidents (
    run_id uuid NOT NULL REFERENCES platform_runs(id),
    id uuid NOT NULL,
    geom geography(Point,4326) NOT NULL,
    record jsonb NOT NULL,
    PRIMARY KEY (run_id,id)
);
CREATE INDEX incident_geometry ON platform_incidents USING gist(geom);
-- Ordinary indexed tables intentionally work without TimescaleDB.
-- Versioned source/weather state and replay metadata also live in the snapshot.
