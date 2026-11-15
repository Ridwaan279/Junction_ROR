export type ReconObservation = {
  sequence: number; simulation: true;
  state: "assigned" | "travelling" | "observing" | "returning" | "complete" | "failed";
  observed_at: string; sim_time_s: number | null;
  drone_position_enu_m: [number, number, number] | null;
  drone_position_lon_lat: [number, number] | null;
  scene_is_geographic: false;
  camera_status: "available" | "no_camera" | "stale";
  detection_method: "simulator_state"; note: string;
};
export type ReconTask = {
  id: string; run_id: string; epoch: number; simulation: true; drone_id: string;
  assigned_at: string; event_stale: boolean; telemetry_stale: boolean; frame_stale: boolean;
  requested_action: "observe" | "return";
  incident: { id: string; title: string; position: [number, number] | null; first_event_at: string | null; updated_at: string; evidence_ids?: string[] };
  reports: { source_url: string; source_id: string; origin_group: string; event_at: string | null; captured_at: string; quality_flags: string[] }[];
  observation: ReconObservation | null;
  frame: { sequence: number; captured_at: string; sim_time_s: number | null; drone_position_enu_m: [number, number, number] | null; image_url: string; origin: "gazebo_camera_render" } | null;
};
export function staleAt(at: string | null | undefined, now: number) {
  return !at || !Number.isFinite(Date.parse(at)) || now - Date.parse(at) > 10_000 || Date.parse(at) - now > 5000;
}
export function cameraState(task: ReconTask, now: number) {
  if (!task.frame) return "no_camera";
  if (staleAt(task.frame.captured_at, now) || staleAt(task.observation?.observed_at, now) || task.observation?.camera_status !== "available") return "stale";
  return "available";
}
