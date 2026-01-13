#!/usr/bin/env node
/**
 * Postinstall script - downloads the MiniMax CLI binary for the current platform.
 */

const https = require("https");
const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

const VERSION = "0.2.0";
const REPO = "Hmbown/MiniMax-CLI";

const PLATFORMS = {
  "linux-x64": "minimax-linux-x64",
  "darwin-arm64": "minimax-macos-arm64",
  "darwin-x64": "minimax-macos-x64",
  "win32-x64": "minimax-windows-x64.exe",
};

async function main() {
  const platform = `${process.platform}-${process.arch}`;
  const assetName = PLATFORMS[platform];

  if (!assetName) {
    console.error(`Unsupported platform: ${platform}`);
    console.error(`Supported: ${Object.keys(PLATFORMS).join(", ")}`);
    process.exit(1);
  }

  const binDir = path.join(__dirname, "bin");
  const binName = process.platform === "win32" ? "minimax.exe" : "minimax";
  const binPath = path.join(binDir, binName);

  // Skip if already exists
  if (fs.existsSync(binPath)) {
    console.log(`MiniMax CLI already installed at ${binPath}`);
    return;
  }

  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${assetName}`;
  console.log(`Downloading MiniMax CLI v${VERSION}...`);

  fs.mkdirSync(binDir, { recursive: true });

  await download(url, binPath);

  // Make executable on Unix
  if (process.platform !== "win32") {
    fs.chmodSync(binPath, 0o755);
  }

  console.log(`Installed MiniMax CLI to ${binPath}`);
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);

    function doRequest(requestUrl) {
      https
        .get(requestUrl, (response) => {
          // Handle redirects
          if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
            doRequest(response.headers.location);
            return;
          }

          if (response.statusCode !== 200) {
            reject(new Error(`Download failed: HTTP ${response.statusCode}`));
            return;
          }

          response.pipe(file);
          file.on("finish", () => {
            file.close();
            resolve();
          });
        })
        .on("error", (err) => {
          fs.unlink(dest, () => {});
          reject(err);
        });
    }

    doRequest(url);
  });
}

main().catch((err) => {
  console.error("Failed to install MiniMax CLI:", err.message);
  process.exit(1);
});
