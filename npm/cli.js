#!/usr/bin/env node
/**
 * CLI wrapper - executes the downloaded MiniMax binary.
 */

const { spawn } = require("child_process");
const path = require("path");
const fs = require("fs");

const binDir = path.join(__dirname, "bin");
const binName = process.platform === "win32" ? "minimax.exe" : "minimax";
const binPath = path.join(binDir, binName);

// Check for override
const override = process.env.MINIMAX_CLI_PATH;
const effectivePath = override && fs.existsSync(override) ? override : binPath;

if (!fs.existsSync(effectivePath)) {
  console.error("MiniMax CLI binary not found.");
  console.error("Try reinstalling: npm install -g minimax-cli");
  process.exit(1);
}

// Spawn the binary with all arguments
const child = spawn(effectivePath, process.argv.slice(2), {
  stdio: "inherit",
});

child.on("error", (err) => {
  console.error("Failed to start MiniMax CLI:", err.message);
  process.exit(1);
});

child.on("exit", (code) => {
  process.exit(code ?? 0);
});
