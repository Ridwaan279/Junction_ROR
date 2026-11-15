"""Adapt an EmberWatch ``Snapshot`` (contracts/snapshot.schema.json) into ONE simulated mission.

Input is the same JSON the web app consumes: ``GET /api/snapshot?run_id=...`` or
``contracts/fixtures/snapshot.json``.  Only incident/evidence/weather fields that exist in
the contract are read.  Anything the snapshot does not state stays ``None`` (JSON null):
we never fill in wind, smoke, event time, etc.

The mission is a *simulation request*.  It does not dispatch anything real.
"""

from __future__ import annotations

import json
import math
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass, field
from typing import Any

from .geo import geodetic_to_local_ned

# Mainland Portugal bbox from AGENTS.md: [west, south, east, north]
PORTUGAL_BBOX = (-9.6, 36.9, -6.1, 42.2)
MISSION_SCHEMA = "emberwatch.sim_mission.v1"
SIM_LABEL = "SIMULATED drone mission - not a real dispatch, no real aircraft"


class MissionError(ValueError):
    """The snapshot cannot be turned into a mission (reported, never papered over)."""


@dataclass
class ReportRef:
    evidence_id: str
    source_id: str | None
    source_url: str | None
    source_name: str | None
    attribution: str | None
    mode: str | None
    event_at: str | None
    published_at: str | None
    captured_at: str | None
    available_at: str | None
    quality_flags: list[str] = field(default_factory=list)


@dataclass
class Conditions:
    """Fire-ground conditions. All None unless the snapshot carries a value.

    Wind is copied from an *observed* (non-forecast) Weather item only, with its station
    distance to the incident so nobody mistakes a station 40 km away for on-scene wind.
    """

    wind_speed_mps: float | None = None
    wind_from_deg: float | None = None
    weather_valid_at: str | None = None
    weather_station_id: str | None = None
    weather_station_distance_m: float | None = None
    weather_source_url: str | None = None
    smoke: Any = None  # not in the contract; always unknown


@dataclass
class Mission:
    schema: str
    label: str
    simulation: bool
    run_id: str | None
    data_mode: str | None  # synthetic | historical | live, copied from the incident
    incident_id: str
    title: str | None
    location_name: str | None
    target_lon: float
    target_lat: float
    target_uncertainty_m: float | None
    reports: list[ReportRef]
    conditions: Conditions
    drone_count: int = 1

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def _in_portugal(lon: float, lat: float) -> bool:
    w, s, e, n = PORTUGAL_BBOX
    return w <= lon <= e and s <= lat <= n


def _num(v: Any) -> float | None:
    return float(v) if isinstance(v, (int, float)) and not isinstance(v, bool) else None


MAX_WEATHER_STATION_DISTANCE_M = 30_000.0


def _conditions(snapshot: dict[str, Any], lon: float, lat: float,
                max_station_m: float = MAX_WEATHER_STATION_DISTANCE_M) -> Conditions:
    """Wind from the NEAREST station (<= max_station_m) that has an observed (forecast == false)
    wind value, using that station's latest observation. Otherwise everything stays unknown.
    Records without a position are never used (distance unknown)."""
    best: tuple[float, str, dict[str, Any]] | None = None
    for w in snapshot.get("weather") or []:
        if not isinstance(w, dict) or w.get("forecast") is not False:
            continue
        if _num(w.get("wind_speed_mps")) is None or _num(w.get("wind_from_deg")) is None:
            continue
        wpos = w.get("position")
        if not (isinstance(wpos, list) and len(wpos) == 2 and all(_num(p) is not None for p in wpos)):
            continue
        n, e = geodetic_to_local_ned(float(wpos[1]), float(wpos[0]), lat, lon)
        dist = math.hypot(n, e)
        if dist > max_station_m:
            continue
        key = (round(dist, 1), str(w.get("valid_at") or ""))
        # nearest station first; for the same station (same distance) the latest observation
        if best is None or key[0] < best[0] or (key[0] == best[0] and key[1] > best[1]):
            best = (key[0], key[1], w)
    if best is None:
        return Conditions()
    dist, _, w = best
    prov = w.get("provenance") or {}
    return Conditions(
        wind_speed_mps=_num(w.get("wind_speed_mps")),
        wind_from_deg=_num(w.get("wind_from_deg")),
        weather_valid_at=w.get("valid_at"),
        weather_station_id=w.get("station_id"),
        weather_station_distance_m=dist,
        weather_source_url=prov.get("source_url") if isinstance(prov, dict) else None,
    )


def mission_from_snapshot(snapshot: dict[str, Any], incident_id: str | None = None) -> Mission:
    incidents = [i for i in snapshot.get("incidents") or [] if isinstance(i, dict)]
    if not incidents:
        # An empty list may be a failed/partial scrape: never treat as "no fires".
        raise MissionError("snapshot has no incidents; nothing to dispatch (not evidence of zero fires)")
    if incident_id is not None:
        matches = [i for i in incidents if i.get("id") == incident_id]
        if not matches:
            raise MissionError(f"incident {incident_id!r} not in snapshot")
        incident = matches[0]
    else:
        incident = incidents[0]

    pos = incident.get("position")
    if not (isinstance(pos, list) and len(pos) == 2 and all(_num(p) is not None for p in pos)):
        raise MissionError(f"incident {incident.get('id')!r} has no [lon, lat] position")
    lon, lat = float(pos[0]), float(pos[1])
    if not _in_portugal(lon, lat):
        raise MissionError(
            f"position [{lon}, {lat}] outside mainland Portugal bbox; refusing (check [lon, lat] order)"
        )

    evidence_by_id = {e.get("id"): e for e in snapshot.get("evidence") or [] if isinstance(e, dict)}
    sources_by_id = {s.get("id"): s for s in snapshot.get("sources") or [] if isinstance(s, dict)}
    reports: list[ReportRef] = []
    for eid in incident.get("evidence_ids") or []:
        ev = evidence_by_id.get(eid)
        if ev is None:
            continue
        prov = ev.get("provenance") or {}
        src = sources_by_id.get(prov.get("source_id")) or {}
        reports.append(
            ReportRef(
                evidence_id=eid,
                source_id=prov.get("source_id"),
                source_url=prov.get("source_url"),
                source_name=src.get("name"),
                attribution=src.get("attribution"),
                mode=ev.get("mode"),
                event_at=prov.get("event_at"),
                published_at=prov.get("published_at"),
                captured_at=prov.get("captured_at"),
                available_at=prov.get("available_at"),
                quality_flags=list(prov.get("quality_flags") or []),
            )
        )
    if not any(r.source_url for r in reports):
        raise MissionError(f"incident {incident.get('id')!r} has no evidence with a source_url; refusing uncited mission")

    scenario = snapshot.get("scenario") or {}
    return Mission(
        schema=MISSION_SCHEMA,
        label=SIM_LABEL,
        simulation=True,
        run_id=incident.get("run_id") or scenario.get("run_id"),
        data_mode=incident.get("mode") or scenario.get("mode"),
        incident_id=str(incident.get("id")),
        title=incident.get("title"),
        location_name=incident.get("location_name"),
        target_lon=lon,
        target_lat=lat,
        target_uncertainty_m=_num(incident.get("uncertainty_m")),
        reports=reports,
        conditions=_conditions(snapshot, lon, lat),
    )


def load_snapshot_file(path: str) -> dict[str, Any]:
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def fetch_snapshot(api_base: str, run_id: str, timeout_s: float = 5.0) -> dict[str, Any]:
    url = f"{api_base.rstrip('/')}/api/snapshot?run_id={urllib.parse.quote(run_id)}"
    with urllib.request.urlopen(url, timeout=timeout_s) as resp:  # noqa: S310 (local service)
        return json.load(resp)
