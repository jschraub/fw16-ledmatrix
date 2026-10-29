import { expect, test } from "vitest";
import { safeFailure } from "./connection";

test("diagnostics whitelist codes and replace arbitrary exception or server text", () => {
  const known = safeFailure({ code: "access_denied", message: "/private/token=secret" });
  expect(known.code).toBe("access_denied");
  expect(known.message).not.toContain("secret");
  for (const error of ["/private/token=secret", { code: "ENOENT", message: "/private/token=secret" }, { code: "__proto__", message: "secret" }]) {
    const failure = safeFailure(error);
    expect(failure.code).toBe("bridge_unavailable");
    expect(failure.message).not.toContain("secret");
  }
});
