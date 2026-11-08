#!/usr/bin/env node
// Checks that this machine can run the local development stack, before `just up` fails halfway.
//
//   node infra/doctor.mjs        (or: just doctor)
//
// Checks: Node, Docker CLI, Docker daemon, Docker Compose, the .env file, and that every host port
// the stack publishes can be bound on 127.0.0.1. Exits 1 if anything required is wrong.
// No dependencies beyond Node itself, so it behaves the same on Windows, macOS and Linux.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import net from "node:net";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const composeFile = join(root, "infra", "docker-compose.yml");
const MIN_NODE_MAJOR = 22;
const MIN_COMPOSE = [2, 20, 0];

let failures = 0;
let warnings = 0;
const ok = (msg) => console.log(`  ok    ${msg}`);
const warn = (msg, hint) => {
  warnings++;
  console.log(`  warn  ${msg}${hint ? `\n          ${hint}` : ""}`);
};
const fail = (msg, hint) => {
  failures++;
  console.log(`  FAIL  ${msg}${hint ? `\n          ${hint}` : ""}`);
};

/** Runs a command and returns trimmed stdout, or null if it is missing or exits non-zero. */
function run(command, args) {
  try {
    return execFileSync(command, args, { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"], timeout: 20000 }).trim();
  } catch {
    return null;
  }
}

/** Reads KEY=VALUE pairs from .env (comments and blank lines ignored). The shell environment wins. */
function loadEnv() {
  const values = {};
  const file = join(root, ".env");
  if (existsSync(file)) {
    for (const line of readFileSync(file, "utf8").split(/\r?\n/)) {
      const match = /^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$/.exec(line);
      if (match && !line.trimStart().startsWith("#")) values[match[1]] = match[2];
    }
  }
  return { ...values, ...process.env };
}

const compareVersions = (a, b) => {
  for (let i = 0; i < 3; i++) if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) - (b[i] ?? 0);
  return 0;
};

/** Resolves to null if 127.0.0.1:port can be bound, otherwise the error code (EADDRINUSE, EACCES...). */
const tryBind = (port) =>
  new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", (error) => resolve(error.code ?? "UNKNOWN"));
    server.listen(port, "127.0.0.1", () => server.close(() => resolve(null)));
  });

console.log("Tools");
const nodeMajor = Number(process.versions.node.split(".")[0]);
if (nodeMajor >= MIN_NODE_MAJOR) ok(`Node ${process.versions.node}`);
else fail(`Node ${process.versions.node} is older than ${MIN_NODE_MAJOR}`, "Install Node 22 or later (https://nodejs.org or nvm).");

const dockerVersion = run("docker", ["--version"]);
let daemonUp = false;
if (!dockerVersion) {
  fail("Docker CLI not found", "Install Docker Desktop (Windows, macOS) or Docker Engine with the Compose plugin (Linux).");
} else {
  ok(dockerVersion);
  daemonUp = run("docker", ["info", "--format", "{{.ServerVersion}}"]) !== null;
  if (daemonUp) ok("Docker daemon is running");
  else fail("Docker daemon is not reachable", "Start Docker Desktop (or `sudo systemctl start docker`) and run this again.");

  const composeVersion = run("docker", ["compose", "version", "--short"]);
  const parsed = composeVersion?.replace(/^v/, "").split(".").map((part) => parseInt(part, 10));
  if (!parsed || parsed.some(Number.isNaN)) fail("Docker Compose v2 plugin not found", "Update Docker Desktop, or install docker-compose-plugin.");
  else if (compareVersions(parsed, MIN_COMPOSE) < 0) fail(`Docker Compose ${composeVersion} is older than ${MIN_COMPOSE.join(".")}`, "Update Docker.");
  else ok(`Docker Compose ${composeVersion}`);
}

console.log("\nConfiguration");
if (existsSync(join(root, ".env"))) ok(".env exists");
else warn(".env does not exist yet", "`just up` copies .env.example to .env the first time.");
const env = loadEnv();
if (!env.APP_OWNER_PASSWORD || !env.APP_RUNTIME_PASSWORD) {
  warn(
    "APP_OWNER_PASSWORD / APP_RUNTIME_PASSWORD are not set, so the app_owner and app_runtime roles will not be created",
    "Copy them from .env.example into .env, then recreate the volume (`just reset`).",
  );
}

console.log("\nHost ports (127.0.0.1)");
const ports = [
  ["POSTGRES_PORT", 5432, "PostgreSQL"],
  ["REDIS_PORT", 6379, "Redis"],
  ["MINIO_PORT", 9000, "MinIO S3 API"],
  ["MINIO_CONSOLE_PORT", 9001, "MinIO console"],
  ["MAILPIT_SMTP_PORT", 1025, "Mailpit SMTP"],
  ["MAILPIT_UI_PORT", 8025, "Mailpit web UI"],
];
// If the stack is already up, its own containers hold these ports; that is fine.
// `just` exports COMPOSE_PROJECT_NAME; run directly, the default project name is used instead.
const project = process.env.COMPOSE_PROJECT_NAME ? ["-p", process.env.COMPOSE_PROJECT_NAME] : [];
const running = daemonUp ? run("docker", ["compose", ...project, "-f", composeFile, "ps", "--status", "running", "-q"]) : null;
if (running) {
  ok("the local stack is already running, so its ports are in use by design (skipping the bind checks)");
} else {
  for (const [variable, fallback, label] of ports) {
    const port = Number(env[variable] ?? fallback);
    const code = await tryBind(port);
    if (code === null) ok(`${port} is free (${label})`);
    else if (code === "EADDRINUSE") {
      fail(`${port} is already in use (${label})`, `Stop whatever listens on it, or set ${variable} in .env to another port.`);
    } else if (code === "EACCES") {
      fail(
        `${port} cannot be bound: permission denied (${label})`,
        `On Windows this is usually a reserved range (see: netsh interface ipv4 show excludedportrange protocol=tcp). Set ${variable} in .env to a port outside it.`,
      );
    } else fail(`${port} cannot be bound: ${code} (${label})`, `Set ${variable} in .env to another port.`);
  }
}

console.log(
  failures === 0
    ? `\nAll checks passed${warnings ? ` (${warnings} warning${warnings > 1 ? "s" : ""})` : ""}.`
    : `\n${failures} check${failures > 1 ? "s" : ""} failed.`,
);
process.exit(failures === 0 ? 0 : 1);
