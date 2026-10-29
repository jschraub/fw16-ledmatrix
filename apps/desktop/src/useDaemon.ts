import { useCallback, useEffect, useRef, useState } from "react";
import type { Hello } from "./bindings/Hello";
import type { Snapshot } from "./bindings/Snapshot";
import type { Failure } from "./bindings/Failure";
import { safeFailure, startDaemon, subscribe } from "./connection";

// Own only connection state. Drafts/settings in callers are never reset by reconnect.
export function useDaemon() {
  const [hello, setHello] = useState<Hello | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [error, setError] = useState<Failure | null>(null);
  const [connecting, setConnecting] = useState(true);
  const [starting, setStarting] = useState(false);
  const generation = useRef(0);

  const clearConnection = useCallback(() => { setHello(null); setSnapshot(null); }, []);
  const connect = useCallback(() => {
    const current = ++generation.current;
    setConnecting(true);
    setError(null);
    clearConnection();
    void subscribe((event) => {
      if (current !== generation.current) return;
      if (event.type === "hello") { setHello(event.hello); setConnecting(false); }
      else if (event.type === "frame") { setSnapshot(event.snapshot); }
      else { setError(safeFailure(event)); setConnecting(false); clearConnection(); }
    }).catch((reason: unknown) => {
      if (current !== generation.current) return;
      setError(safeFailure(reason));
      setConnecting(false);
      clearConnection();
    });
  }, [clearConnection]);

  const start = useCallback(async () => {
    const current = ++generation.current;
    setStarting(true);
    try {
      await startDaemon();
      if (current === generation.current) connect();
    } catch (reason) {
      if (current === generation.current) setError(safeFailure(reason));
    } finally { setStarting(false); }
  }, [connect]);

  useEffect(() => {
    connect();
    return () => { generation.current++; };
  }, [connect]);

  return { hello, snapshot, error, connecting, starting, connect, start };
}
