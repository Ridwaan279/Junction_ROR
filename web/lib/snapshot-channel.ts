import type { EventEnvelope, Snapshot } from "./contracts";
import { applyEvent, type StreamState } from "./model";
/** One ordering authority shared by HTTP resync, commands and the event stream. */
export class SnapshotChannel {
  state: StreamState | null = null;
  private sequence = 0;
  constructor(snapshot?: Snapshot) {
    if (snapshot) this.replace(snapshot);
  }
  beginRequest(): number {
    return ++this.sequence;
  }
  isCurrent(ticket: number): boolean {
    return ticket === this.sequence;
  }
  replace(snapshot: Snapshot): StreamState {
    this.state = { snapshot, needsResync: false, seen: [] };
    return this.state;
  }
  acceptHttp(snapshot: Snapshot, ticket: number): StreamState | null {
    const current = this.state?.snapshot.scenario;
    if (
      !this.isCurrent(ticket) ||
      (current &&
        (snapshot.scenario.run_id !== current.run_id ||
          snapshot.scenario.epoch < current.epoch))
    )
      return null;
    return this.replace(snapshot);
  }
  acceptStream(event: EventEnvelope): StreamState | null {
    if (!this.state) return null;
    const next = applyEvent(this.state, event);
    if (next === this.state) return null;
    // Stream arrival wins even when replay time and epoch are unchanged.
    this.sequence++;
    this.state = next;
    return next;
  }
}
