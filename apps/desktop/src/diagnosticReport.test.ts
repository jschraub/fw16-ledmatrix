import { expect, test } from "vitest";
import { diagnosticReport } from "./diagnosticReport";
import type { Snapshot } from "./bindings/Snapshot";

test("diagnoses missing and failing assignments without exporting source or device identifiers", () => {
  const privateText = "PRIVATE-session-token-path";
  const snapshot = {
    panels: [{name:"left", binding:privateText}, {name:"right",binding:"missing-id"}],
    assistants:{health:[{provider:"claude",status:privateText,detail:privateText}],sessions:[{session_id:privateText}]},
    display_sources:{connected:[privateText],backlights:{[privateText]:50}},
  } as unknown as Snapshot;
  const report = diagnosticReport(snapshot, undefined,[{port:privateText,location:privateText,firmware:privateText,status:`device write failed ${privateText}`}]);
  expect(report).not.toContain(privateText);
  expect(JSON.parse(report).panels).toEqual([{role:"left",state:"write failed"},{role:"right",state:"missing"}]);
  expect(JSON.parse(diagnosticReport(null,privateText,null)).connection).toBe("unknown");
});
