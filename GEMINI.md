# Awaria Project Context

## Project Overview

**Name**: Awaria
**Description**: A cross-platform desktop and mobile application built with Tauri to notify users about Tauron power outages.
**Identifier**: `xyz.eremef.awaria`

## Architecture

- **Frontend**: Located in `public/`. Seens to use vanilla HTML/JS/CSS (based on `frontendDist` configuration).
- **Backend (Core)**: Rust-based Tauri backend located in `src-tauri/`.
- **Mobile**: Android and iOS support enabled via Tauri mobile.

## Key Configuration Files

- **`src-tauri/tauri.conf.json`**: Main Tauri configuration file.
- **`package.json`**: Node.js dependencies and scripts.
- **`src-tauri/Cargo.toml`**: Rust dependencies and workspace configuration.
- **`.github/workflows/`**: CI/CD pipelines (e.g., `release.yml`).

## Development Commands

- `npm run tauri dev`: Start desktop development server.
- `npm run android:dev`: Start Android development server.
- `npm run build`: Build web assets and desktop application.
- `npm run android:build`: Build Android application.

## User Rules

- When you bump version, update it in `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `package.json` using semver versioning, even when git tag is in other format, like 1.0.0b
- when bumping `src-tauri/tauri.conf.json` use only X.X.X format, without any additional suffixes
