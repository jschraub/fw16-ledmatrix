import { Channel, invoke } from "@tauri-apps/api/core";
import type { Event } from "./bindings/Event";
import type { Failure } from "./bindings/Failure";
import type { ErrorCode } from "./bindings/ErrorCode";
import failures from "./bindings/failures.json";

const bridgeFailure = failures.bridge_unavailable as Failure;

function safeFailure(reason: unknown): Failure {
  // Never stringify arbitrary exceptions: they may include paths or payloads.
  if (typeof reason === "object" && reason !== null && "code" in reason
      && typeof reason.code === "string" && Object.hasOwn(failures, reason.code)) {
    return failures[reason.code as ErrorCode] as Failure;
  }
  return bridgeFailure;
}

export async function subscribe(onEvent: (event: Event) => void): Promise<void> {
  const subscriptionId = crypto.randomUUID();
  const channel = new Channel<Event>();
  channel.onmessage = (event) => {
    onEvent(event);
    if (event.type === "frame") {
      // A stalled WebView cannot accumulate an unbounded native frame queue.
      void invoke("acknowledge", { subscriptionId, sequence: event.snapshot.sequence }).catch(() => {});
    }
  };
  try {
    await invoke("subscribe", { onEvent: channel, subscriptionId });
  } catch (reason) {
    throw safeFailure(reason);
  }
}

export async function startDaemon(): Promise<void> {
  try { await invoke("start_daemon"); }
  catch (reason) { throw safeFailure(reason); }
}

export { safeFailure };
