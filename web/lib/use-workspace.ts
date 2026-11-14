"use client";
import { useCallback, useEffect, useRef, useState } from "react";
import type { EventEnvelope, Evidence, Snapshot } from "./contracts";
import { applyEvent, parseSnapshot, replayAt, type StreamState } from "./model";
export type Connection =
  "loading" | "local" | "connecting" | "connected" | "disconnected";
const apiBase = (process.env.NEXT_PUBLIC_API_BASE_URL ?? "").replace(/\/$/, "");
const eventTypes = [
  "run.reset",
  "scenario.tick",
  "source.run",
  "evidence.new",
  "incident.upsert",
  "weather.update",
  "forecast.ready",
  "drone.plan",
  "drone.replan",
  "drone.update",
  "briefing.ready",
  "resync.required",
];
export function useWorkspace() {
  const [state, setState] = useState<StreamState | null>(null);
  const [connection, setConnection] = useState<Connection>("loading");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const current = useRef<StreamState | null>(null),
    full = useRef<Snapshot | null>(null);
  const sequence = useRef(0);
  const commit = useCallback((snapshot: Snapshot) => {
    const next = { snapshot, needsResync: false, seen: [] };
    current.current = next;
    setState(next);
  }, []);
  const resync = useCallback(async () => {
    const request = ++sequence.current;
    const run = current.current?.snapshot.scenario.run_id;
    if (!run || !apiBase) return;
    try {
      const response = await fetch(
        apiBase + "/api/snapshot?run_id=" + encodeURIComponent(run),
        { cache: "no-store", signal: AbortSignal.timeout(10000) },
      );
      if (!response.ok) throw new Error("Snapshot request failed");
      const snapshot = parseSnapshot(await response.json());
      if (request !== sequence.current || snapshot.scenario.run_id !== run)
        return;
      if (
        current.current &&
        snapshot.scenario.epoch < current.current.snapshot.scenario.epoch
      )
        return;
      commit(snapshot);
      setError(null);
    } catch {
      if (request === sequence.current) {
        setConnection("disconnected");
        setError("connection");
      }
    }
  }, [commit]);
  useEffect(() => {
    let cancelled = false;
    const controller = new AbortController();
    (async () => {
      try {
        let snapshot: Snapshot;
        if (apiBase) {
          const scenariosResponse = await fetch(apiBase + "/api/scenarios", {
            signal: controller.signal,
            cache: "no-store",
          });
          if (!scenariosResponse.ok) throw new Error("Scenario request failed");
          const scenarios = await scenariosResponse.json();
          if (!Array.isArray(scenarios) || !scenarios[0]?.run_id)
            throw new Error("No scenario available");
          const response = await fetch(
            apiBase +
              "/api/snapshot?run_id=" +
              encodeURIComponent(scenarios[0].run_id),
            { signal: controller.signal, cache: "no-store" },
          );
          if (!response.ok) throw new Error("Snapshot request failed");
          snapshot = parseSnapshot(await response.json());
        } else {
          const [initial, upcoming] = await Promise.all([
            fetch("/snapshot.json", { signal: controller.signal }),
            fetch("/replay-evidence.json", { signal: controller.signal }),
          ]);
          if (!initial.ok || !upcoming.ok) throw new Error("Fixture missing");
          snapshot = parseSnapshot(await initial.json());
          const later = (await upcoming.json()) as Evidence[];
          full.current = parseSnapshot({
            ...snapshot,
            evidence: [...snapshot.evidence, ...later],
          });
          snapshot = replayAt(
            full.current,
            snapshot.scenario.current_at,
            snapshot.scenario.epoch,
          );
        }
        if (cancelled) return;
        commit(snapshot);
        setConnection(apiBase ? "connecting" : "local");
      } catch {
        if (!cancelled) {
          setConnection("disconnected");
          setError("connection");
        }
      }
    })();
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [commit]);
  const runId = state?.snapshot.scenario.run_id;
  useEffect(() => {
    if (!apiBase || !runId) return;
    const stream = new EventSource(
      apiBase + "/api/events?run_id=" + encodeURIComponent(runId),
    );
    stream.onopen = () => {
      setConnection("connected");
      void resync();
    };
    stream.onerror = () => {
      setConnection("disconnected");
      setError("connection");
    };
    const receive = (message: MessageEvent) => {
      try {
        const event = JSON.parse(message.data) as EventEnvelope;
        if (
          typeof event.id !== "string" ||
          typeof event.type !== "string" ||
          !Number.isInteger(event.epoch) ||
          !current.current
        )
          throw new Error("Invalid event");
        const next = applyEvent(current.current, event);
        if (next === current.current) return;
        current.current = next;
        setState(next);
        if (next.needsResync) void resync();
        else {
          setConnection("connected");
          setError(null);
        }
      } catch {
        void resync();
      }
    };
    eventTypes.forEach((type) =>
      stream.addEventListener(type, receive as EventListener),
    );
    return () => stream.close();
  }, [runId, resync]);
  useEffect(() => {
    if (apiBase) return;
    const timer = setInterval(() => {
      const snapshot = current.current?.snapshot,
        source = full.current;
      if (!snapshot?.scenario.playing || !source) return;
      const nextTime = Math.min(
        Date.parse(snapshot.scenario.end_at),
        Date.parse(snapshot.scenario.current_at) +
          1000 * snapshot.scenario.speed,
      );
      const next = replayAt(
        source,
        new Date(nextTime).toISOString(),
        snapshot.scenario.epoch,
      );
      next.scenario.playing = nextTime < Date.parse(snapshot.scenario.end_at);
      next.scenario.speed = snapshot.scenario.speed;
      commit(next);
    }, 1000);
    return () => clearInterval(timer);
  }, [commit]);
  const command = useCallback(
    async (action: "play" | "pause" | "seek" | "reset", at?: string) => {
      const snapshot = current.current?.snapshot;
      if (!snapshot) return;
      setBusy(true);
      setError(null);
      try {
        if (apiBase) {
          const request = ++sequence.current;
          const response = await fetch(
            apiBase +
              "/api/scenarios/" +
              encodeURIComponent(snapshot.scenario.id) +
              "/" +
              action +
              "?run_id=" +
              encodeURIComponent(snapshot.scenario.run_id),
            {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify(
                action === "seek"
                  ? { at }
                  : action === "play"
                    ? { speed: snapshot.scenario.speed }
                    : {},
              ),
              signal: AbortSignal.timeout(10000),
            },
          );
          if (!response.ok) throw new Error("Command failed");
          const next = parseSnapshot(await response.json());
          if (
            request === sequence.current &&
            next.scenario.run_id === snapshot.scenario.run_id &&
            next.scenario.epoch >=
              (current.current?.snapshot.scenario.epoch ??
                snapshot.scenario.epoch)
          )
            commit(next);
        } else if (full.current) {
          const reset = action === "seek" || action === "reset";
          const next = replayAt(
            full.current,
            action === "reset"
              ? snapshot.scenario.start_at
              : (at ?? snapshot.scenario.current_at),
            snapshot.scenario.epoch + (reset ? 1 : 0),
          );
          next.scenario.playing =
            action === "play" ||
            (!reset && action !== "pause" && snapshot.scenario.playing);
          next.scenario.speed = snapshot.scenario.speed;
          commit(next);
        }
      } catch {
        setError("command");
        if (apiBase) setConnection("disconnected");
      } finally {
        setBusy(false);
      }
    },
    [commit],
  );
  const dispatch = useCallback(
    async (incidentId: string) => {
      const snapshot = current.current?.snapshot;
      if (!snapshot || !apiBase) return;
      setBusy(true);
      setError(null);
      const request = ++sequence.current;
      try {
        const response = await fetch(
          apiBase +
            "/api/incidents/" +
            encodeURIComponent(incidentId) +
            "/dispatch?run_id=" +
            encodeURIComponent(snapshot.scenario.run_id),
          {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: "{}",
            signal: AbortSignal.timeout(15000),
          },
        );
        if (!response.ok) throw new Error("Simulation failed");
        const next = parseSnapshot(await response.json());
        if (
          request === sequence.current &&
          next.scenario.run_id === snapshot.scenario.run_id &&
          next.scenario.epoch >= (current.current?.snapshot.scenario.epoch ?? 0)
        )
          commit(next);
      } catch {
        setError("command");
      } finally {
        setBusy(false);
      }
    },
    [commit],
  );
  return {
    snapshot: state?.snapshot ?? null,
    connection,
    error,
    busy,
    command,
    resync,
    dispatch: apiBase ? dispatch : undefined,
  };
}
