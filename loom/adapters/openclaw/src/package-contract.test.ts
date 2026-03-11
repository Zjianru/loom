import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const currentDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(currentDir, "..");

describe("package contract", () => {
  it("declares the OpenClaw extension entrypoint for plugin installation", () => {
    const packageJson = JSON.parse(
      readFileSync(resolve(projectRoot, "package.json"), "utf8"),
    ) as {
      openclaw?: { extensions?: string[] };
      scripts?: { "export:extension"?: string };
    };

    expect(packageJson.openclaw?.extensions).toEqual(["./dist/index.js"]);
    expect(packageJson.scripts?.["export:extension"]).toContain("npm run build");
  });
});
