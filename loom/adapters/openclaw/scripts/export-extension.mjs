import { cpSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(scriptDir, "..");
const distDir = resolve(projectRoot, "dist");
const sourceManifest = resolve(projectRoot, "openclaw.plugin.json");
const sourcePackageJson = resolve(projectRoot, "package.json");
const outputDir =
  process.env.LOOM_EXTENSION_OUT_DIR ??
  resolve(projectRoot, "..", "..", "..", "extensions", "loom-openclaw");

if (!existsSync(distDir)) {
  throw new Error("dist/ is missing. Run `npm run build` before exporting.");
}

mkdirSync(outputDir, { recursive: true });
mkdirSync(join(outputDir, "dist"), { recursive: true });
cpSync(distDir, join(outputDir, "dist"), { recursive: true });
cpSync(sourceManifest, join(outputDir, "openclaw.plugin.json"));

const sourcePackage = JSON.parse(readFileSync(sourcePackageJson, "utf8"));
const exportedPackage = {
  name: sourcePackage.name,
  version: sourcePackage.version,
  type: sourcePackage.type,
  main: "dist/index.js",
  openclaw: {
    extensions: ["./dist/index.js"],
  },
};
writeFileSync(
  join(outputDir, "package.json"),
  `${JSON.stringify(exportedPackage, null, 2)}\n`,
);
writeFileSync(
  join(outputDir, "README.md"),
  [
    "# loom-openclaw",
    "",
    "Generated projection from `/Users/codez/.openclaw/loom/adapters/openclaw`.",
    "Do not hand-edit this directory.",
    "",
  ].join("\n"),
);
