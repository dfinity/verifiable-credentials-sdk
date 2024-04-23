import esbuild from "esbuild";
import {
  existsSync,
  mkdirSync,
  readFileSync,
} from "fs";
import { join } from "path";

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
  const mainEntryPoint = `${join(process.cwd(), "src")}/main.ts`;

  // esm output bundles with code splitting
  esbuild
    .build({
      entryPoints: [mainEntryPoint],
      outfile: "dist/index.js",
      bundle: true,
      sourcemap: true,
      minify: true,
      splitting: false,
      format: "esm",
      define: { global: "window" },
      target: ["esnext"],
      platform: "browser",
      conditions: ["browser"],
      external: [
        ...Object.keys(workspacePeerDependencies),
      ],
    })
    .catch(() => process.exit(1));
};

export const build = () => {
  createDistFolder();
  bundleFiles();
};

build();
