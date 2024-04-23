import esbuild from "esbuild";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
} from "node:fs";
import { join } from "node:path";

const peerDependencies = (packageJson) => {
  const json = readFileSync(packageJson, "utf8");
  const { peerDependencies } = JSON.parse(json);
  return peerDependencies ?? {};
};

const workspacePeerDependencies = peerDependencies(
  join(process.cwd(), "package.json"),
);

const dist = join(process.cwd(), "dist");

const createDistFolder = () => {
  if (!existsSync(dist)) {
    mkdirSync(dist);
  }
};

const bundleFiles = () => {
  const entryPoints = readdirSync(join(process.cwd(), "src"))
    .filter(
      (file) =>
        !file.includes("test") &&
        !file.includes("spec") &&
        statSync(join(process.cwd(), "src", file)).isFile(),
    )
    .map((file) => `src/${file}`);

  // esm output bundles with code splitting
  esbuild
    .build({
      entryPoints,
      outdir: "dist",
      bundle: true,
      sourcemap: true,
      minify: true,
      splitting: false,
      format: "esm",
      define: { global: "window" },
      target: ["esnext"],
      platform: "browser",
      conditions: ["browser"],
      external: [...Object.keys(workspacePeerDependencies)],
    })
    .catch(() => process.exit(1));
};

export const build = () => {
  createDistFolder();
  bundleFiles();
};

build();
