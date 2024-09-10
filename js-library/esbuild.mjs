import esbuild from "esbuild";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
} from "node:fs";
import { join } from "node:path";

const PACKAGE_JSON = "package.json";

const readPackageJson = () => {
  const packageJson = join(process.cwd(), PACKAGE_JSON);
  const json = readFileSync(packageJson, "utf8");
  const { peerDependencies, files } = JSON.parse(json);
  return {
    workspacePeerDependencies: peerDependencies ?? {},
    packageJsonFiles: files ?? [],
  };
};

const { workspacePeerDependencies, packageJsonFiles } = readPackageJson();

const dist = join(process.cwd(), "dist");

const createDistFolder = () => {
  if (!existsSync(dist)) {
    mkdirSync(dist);
  }
};

const entryPoints = readdirSync(join(process.cwd(), "src"))
  .filter(
    (file) =>
      !file.includes("test") &&
      !file.includes("spec") &&
      !file.includes("mock") &&
      statSync(join(process.cwd(), "src", file)).isFile(),
  )
  .map((file) => `src/${file}`);

const buildBrowser = () => {
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

const copyFiles = () => {
  const copyFile = (filename) =>
    copyFileSync(join(process.cwd(), filename), join(dist, filename));

  packageJsonFiles.filter((entry) => !entry.includes("*")).forEach(copyFile);

  copyFile(PACKAGE_JSON);
};

const build = () => {
  createDistFolder();
  buildBrowser();
  copyFiles();
};

build();
