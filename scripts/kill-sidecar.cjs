// scripts/kill-sidecar.js
// Kills any orphaned endlessli-sidecar process on Windows.
// On other platforms, it does nothing.
const { execSync } = require('child_process');

if (process.platform === 'win32') {
  try {
    execSync(
      'powershell -Command "Get-Process endlessli-sidecar -ErrorAction SilentlyContinue | Stop-Process -Force; exit 0"',
      { stdio: 'inherit' }
    );
  } catch {
    // No process found, or PowerShell failed.
  }
}