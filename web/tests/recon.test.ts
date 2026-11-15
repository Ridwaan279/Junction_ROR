import { describe, expect, it } from "vitest";
import { cameraState, staleAt, type ReconTask } from "../lib/recon";

describe("recon freshness", () => {
  const now = Date.parse("2026-11-15T06:00:00Z");
  it("never calls missing, invalid or future camera timestamps fresh", () => {
    for (const at of [null, "bad", "2026-11-15T07:00:00Z", "2026-11-15T05:59:49Z"]) expect(staleAt(at, now)).toBe(true);
    expect(staleAt("2026-11-15T05:59:59Z", now)).toBe(false);
  });
  it("marks the retained frame stale even when telemetry continues", () => {
    const task = {
      frame: { captured_at:"2026-11-15T05:59:00Z" },
      observation: { observed_at:"2026-11-15T06:00:00Z", camera_status:"available" },
    } as ReconTask;
    expect(cameraState(task, now)).toBe("stale");
    expect(cameraState({ ...task, frame:null }, now)).toBe("no_camera");
  });
});
