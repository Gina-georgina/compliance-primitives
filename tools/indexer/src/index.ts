/**
 * Entry point — wires config, DB, and Indexer together and runs the
 * poll loop until SIGINT/SIGTERM.
 *
 * Pass `--dry-run` (or set `DRY_RUN=1`) to validate the configuration and
 * RPC connectivity without opening or writing to the database. Exits 0 on
 * success, 1 on any validation failure. Useful for CI pre-flight checks and
 * first-time operator setup.
 */

import { loadConfig } from "./config.js";
import { ComplianceDb } from "./db.js";
import { Indexer } from "./indexer.js";
import { SorobanRpc } from "./rpc.js";

async function dryRun(): Promise<void> {
  console.log("=== compliance-indexer dry-run ===\n");

  // 1. Validate config
  let config;
  try {
    config = loadConfig();
  } catch (err) {
    console.error("Config error:", (err as Error).message);
    process.exit(1);
  }

  console.log("Config:");
  console.log(`  RPC URL:          ${config.rpcUrl}`);
  console.log(`  Network:          ${config.networkPassphrase}`);
  console.log(`  DB path:          ${config.dbPath}`);
  console.log(`  Poll interval:    ${config.pollIntervalMs}ms`);
  console.log(`  Start ledger:     ${config.startLedger}`);
  const contracts: [string, string][] = [
    ["allowlist", config.allowlistContractId],
    ["denylist", config.denylistContractId],
    ["jurisdiction", config.jurisdictionContractId],
    ["multisig", config.multisigContractId],
    ["aggregator", config.aggregatorContractId],
    ["policy-engine", config.policyEngineContractId],
    ["circuit-breaker", config.circuitBreakerContractId],
  ];
  console.log("  Contracts:");
  for (const [name, id] of contracts) {
    console.log(`    ${name.padEnd(16)} ${id || "(not configured)"}`);
  }

  // 2. Test RPC connectivity
  console.log("\nChecking RPC connectivity…");
  try {
    const rpc = new SorobanRpc(config.rpcUrl);
    const ledger = await rpc.getLatestLedger();
    console.log(`  ✓ RPC reachable — latest ledger: ${ledger}`);
  } catch (err) {
    console.error("  ✗ RPC unreachable:", (err as Error).message);
    process.exit(1);
  }

  console.log("\nDry-run complete — config valid, RPC reachable.");
  process.exit(0);
}

async function main(): Promise<void> {
  const isDryRun =
    process.argv.includes("--dry-run") || process.env.DRY_RUN === "1";

  if (isDryRun) {
    await dryRun();
    return;
  }

  const config = loadConfig();
  const db = await ComplianceDb.open(config.dbPath);
  const indexer = new Indexer(config, db);

  function shutdown(signal: string): void {
    console.log(`\nReceived ${signal}, shutting down…`);
    indexer.stop();
    db.close();
    process.exit(0);
  }

  process.on("SIGINT", () => shutdown("SIGINT"));
  process.on("SIGTERM", () => shutdown("SIGTERM"));

  indexer.start();
}

main().catch((err) => {
  console.error("Fatal:", err);
  process.exit(1);
});
