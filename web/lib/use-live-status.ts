"use client";
import { useCallback, useEffect, useRef, useState } from "react";
import { isLiveStatus, LIVE_REFRESH_MS, type LiveStatus } from "./live-data";

export function useLiveStatus() {
  const [data, setData] = useState<LiveStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [receivedAt, setReceivedAt] = useState<string | null>(null);
  const controller = useRef<AbortController | null>(null);
  const mounted = useRef(false);
  const refresh = useCallback(async () => {
    if (controller.current) return;
    const request = new AbortController(); controller.current = request;
    if (mounted.current) setLoading(true);
    try {
      const response = await fetch("/api/live/status", { cache: "no-store", signal: request.signal });
      if (!response.ok) throw new Error("Live source connection unavailable");
      const value: unknown = await response.json();
      if (!isLiveStatus(value)) throw new Error("Invalid live source response");
      if (mounted.current) { setData(value); setError(null); setReceivedAt(new Date().toISOString()); }
    } catch (failure) {
      if (mounted.current && !request.signal.aborted) setError(failure instanceof Error ? failure.message : "Live source connection unavailable");
    } finally {
      if (controller.current === request) { controller.current = null; if (mounted.current) setLoading(false); }
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    const first = setTimeout(() => void refresh(), 0);
    const timer = setInterval(() => { if (document.visibilityState === "visible") void refresh(); }, LIVE_REFRESH_MS);
    return () => { mounted.current = false; clearTimeout(first); clearInterval(timer); controller.current?.abort(); controller.current = null; };
  }, [refresh]);
  return { data, error, loading, receivedAt, refresh };
}
