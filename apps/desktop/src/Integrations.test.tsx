import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { Integrations } from "./Integrations";
afterEach(cleanup);
test("shows metadata freshness separately from activity and does not imply terminal focus", () => {
  render(<Integrations state={{ health: [{ provider: "claude", status: "live", detail: "fixture" }], sessions: [{ provider: "claude", instance_id: "a", session_id: "ses_main", context_percent: 41, busy: true, last_activity_ms: 1000, heartbeat_ms: 2000 }] }} />);
  expect(screen.getByText(/ses_main/)).toHaveTextContent("context 41");
  expect(screen.getByText(/Last activity:/)).toHaveTextContent("1970-01-01T00:00:01.000Z");
  expect(screen.getByText(/Last activity:/)).toHaveTextContent("1970-01-01T00:00:02.000Z");
  expect(screen.getByText(/not terminal focus/)).toBeInTheDocument();
});
