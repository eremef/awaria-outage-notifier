import fs from 'node:fs';

const pkgPath = 'package.json';
const tauriConfPath = 'src-tauri/tauri.conf.json';
const cargoTomlPath = 'src-tauri/Cargo.toml';

function bumpVersion(newVersion) {
  // Update package.json
  const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
  pkg.version = newVersion;
  fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n');
  console.log(`Updated ${pkgPath} to ${newVersion}`);

  // Update tauri.conf.json
  const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf8'));
  tauriConf.version = newVersion;

  // Ensure bundle.android exists
  if (!tauriConf.bundle.android) {
    tauriConf.bundle.android = {};
  }

  // Increment versionCode (logic: use current versionCode + 1 or date-based)
  // For simplicity, let's use a sequential increment or manual if provided.
  // We'll read the current one from build.gradle if needed, but let's manage it in tauri.conf.json now.
  const currentVersionCode = tauriConf.bundle.android.versionCode || 1;
  tauriConf.bundle.android.versionCode = currentVersionCode + 1;

  fs.writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + '\n');
  console.log(`Updated ${tauriConfPath} to version ${newVersion}, versionCode ${tauriConf.bundle.android.versionCode}`);

  // Update Cargo.toml
  let cargoToml = fs.readFileSync(cargoTomlPath, 'utf8');
  cargoToml = cargoToml.replace(/^version = ".*"$/m, `version = "${newVersion}"`);
  fs.writeFileSync(cargoTomlPath, cargoToml);
  console.log(`Updated ${cargoTomlPath} to ${newVersion}`);
}

const args = process.argv.slice(2);
if (args.length === 0) {
  console.error('Please provide a new version (e.g., node scripts/bump-version.mjs 1.0.21)');
  process.exit(1);
}

bumpVersion(args[0]);
