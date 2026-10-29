import { Channel, invoke } from "@tauri-apps/api/core";
import type { Event } from "./bindings/Event";

// The Rust bridge owns the socket/pipe. This UI never opens devices or draws digits.
export async function subscribe(onEvent: (event: Event) => void): Promise<void> {
  const channel = new Channel<Event>();
  channel.onmessage = onEvent;
  await invoke("subscribe", { onEvent: channel });
}
