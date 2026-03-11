import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const currentDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(currentDir, "..");

describe("plugin manifest contract", () => {
  it("keeps bridge.baseUrl and bridge.runtimeRoot as optional overrides so installation can succeed without preseeded config", () => {
    const manifest = JSON.parse(
      readFileSync(resolve(projectRoot, "openclaw.plugin.json"), "utf8"),
    ) as {
      configSchema?: {
        required?: string[];
        properties?: {
          bridge?: {
            required?: string[];
            properties?: Record<string, unknown>;
          };
        };
      };
    };

    expect(manifest.configSchema?.properties?.bridge?.properties).toHaveProperty("baseUrl");
    expect(manifest.configSchema?.properties?.bridge?.properties).toHaveProperty("runtimeRoot");
    expect(manifest.configSchema?.required ?? []).not.toContain("bridge");
    expect(manifest.configSchema?.properties?.bridge?.required ?? []).not.toContain("baseUrl");
    expect(manifest.configSchema?.properties?.bridge?.required ?? []).not.toContain("runtimeRoot");
  });
});
