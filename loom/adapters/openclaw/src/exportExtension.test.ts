import { mkdtempSync, readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { execFileSync } from "node:child_process";
import { describe, expect, it } from "vitest";
import { fileURLToPath } from "node:url";

const projectRoot = dirname(dirname(fileURLToPath(import.meta.url)));

describe("export:extension", () => {
  it("projects plugin manifest and compiled entry into extension output", () => {
    const outputDir = mkdtempSync(join(tmpdir(), "loom-openclaw-export-"));
    execFileSync("npm", ["run", "build"], {
      cwd: projectRoot,
      stdio: "inherit",
    });
    execFileSync("node", ["scripts/export-extension.mjs"], {
      cwd: projectRoot,
      env: { ...process.env, LOOM_EXTENSION_OUT_DIR: outputDir },
      stdio: "inherit",
    });

    const manifest = JSON.parse(
      readFileSync(join(outputDir, "openclaw.plugin.json"), "utf8"),
    );
    expect(manifest.id).toBe("loom-openclaw");
    const packageJson = JSON.parse(
      readFileSync(join(outputDir, "package.json"), "utf8"),
    );
    expect(packageJson.main).toBe("dist/index.js");
  });

  it("replaces stale compiled files in an existing extension output directory", () => {
    const outputDir = mkdtempSync(join(tmpdir(), "loom-openclaw-export-"));
    mkdirSync(join(outputDir, "dist"), { recursive: true });
    writeFileSync(join(outputDir, "dist", "index.js"), "throw new Error('stale artifact');\n");

    execFileSync("npm", ["run", "build"], {
      cwd: projectRoot,
      stdio: "inherit",
    });
    execFileSync("node", ["scripts/export-extension.mjs"], {
      cwd: projectRoot,
      env: { ...process.env, LOOM_EXTENSION_OUT_DIR: outputDir },
      stdio: "inherit",
    });

    const exportedEntry = readFileSync(join(outputDir, "dist", "index.js"), "utf8");
    expect(exportedEntry).toContain("bridge.peer.connecting");
    expect(exportedEntry).not.toContain("stale artifact");
  });
});
