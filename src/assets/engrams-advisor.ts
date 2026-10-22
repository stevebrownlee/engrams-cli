/**
 * engrams pre-edit advisor — omp extension.
 *
 * Intercepts `edit` and `write` tool calls. On the first edit to each file per
 * session, runs `engrams advise <path>` and delivers surfaced schemas and
 * constraints as a non-blocking steer message. Blocks only on error-severity
 * violations, starting the block reason with any matching schemas.
 *
 * Self-disables outside engrams workspaces (no engrams/context.db found in
 * ancestor directories).
 *
 * Installed by: engrams install --harness omp --hooks
 * Or manually: cp into .omp/extensions/engrams-advisor/index.ts
 */
import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";
import { Glob } from "bun";
import { existsSync } from "node:fs";
import { basename, isAbsolute, join, relative, resolve } from "node:path";

export interface AdviseOutput {
  schemas: unknown[];
  constraints: unknown[];
  violations: unknown[];
}

/**
 * Normalize raw `engrams advise` output (parsed JSON object or JSON string)
 * into `{ schemas, constraints, violations }` arrays.
 */
export function parseAdviseOutput(raw: unknown): AdviseOutput {
  const parsed = typeof raw === "string" ? JSON.parse(raw) : raw;
  const data = (parsed ?? {}) as Record<string, unknown>;
  return {
    schemas: Array.isArray(data.schemas) ? data.schemas : [],
    constraints: Array.isArray(data.constraints) ? data.constraints : [],
    violations: Array.isArray(data.violations) ? data.violations : [],
  };
}

function formatSchemaScope(item: Record<string, unknown>): string {
  const scope = (item.scope ?? {}) as { paths?: unknown[]; tags?: unknown[] };
  const paths = Array.isArray(scope.paths) ? scope.paths.map(String) : [];
  const tags = Array.isArray(scope.tags) ? scope.tags.map(String) : [];
  const parts: string[] = [];
  if (paths.length > 0) parts.push(`paths ${paths.join(", ")}`);
  if (tags.length > 0) parts.push(`tags ${tags.join(", ")}`);
  const scopeBody = parts.length > 0 ? parts.join("; ") : "none";

  const matchedBy = (item.matched_by ?? {}) as { via?: unknown; value?: unknown };
  const via = typeof matchedBy.via === "string" ? matchedBy.via.replaceAll("_", " ") : "";
  const value = typeof matchedBy.value === "string" ? matchedBy.value : "";
  if (via && value) {
    return `scope: ${scopeBody} (matched: ${via} ${value})`;
  }
  if (via) {
    return `scope: ${scopeBody} (matched: ${via})`;
  }
  return `scope: ${scopeBody}`;
}

function formatSchemasSection(path: string, schemas: unknown[]): string {
  if (schemas.length === 0) return "";
  let section = `engrams schemas for ${path}:\n`;
  for (const s of schemas) {
    const item = s as Record<string, unknown>;
    section += `  [schema] ${item.name ?? "?"}\n`;
    const statement = typeof item.statement === "string" ? item.statement.trim() : "";
    if (statement) {
      section += `    ${statement}\n`;
    }
    const summary = typeof item.summary === "string" ? item.summary.trim() : "";
    if (summary) {
      section += `    ${summary}\n`;
    }
    section += `    ${formatSchemaScope(item)}\n`;
  }
  return section;
}

/**
 * Format the non-blocking pre-edit steer message from surfaced schemas and
 * file-level constraints. Schemas render ahead of constraints; empty sections
 * are omitted.
 */
export function formatSteer(
  path: string,
  schemas: unknown[],
  constraints: unknown[] = [],
): string {
  let msg = formatSchemasSection(path, schemas);
  if (constraints.length > 0) {
    msg += `engrams constraints for ${path}:\n`;
    for (const c of constraints) {
      const item = c as Record<string, unknown>;
      const type = String(item.type ?? "?");
      if (type === "pattern") {
        const sev = String(item.severity ?? "info");
        msg += `  [${sev}] ${item.name ?? "?"}`;
        if (item.check_kind && item.check_expr) {
          msg += ` — ${item.check_kind}: /${item.check_expr}/`;
        }
        msg += "\n";
        if (item.description) msg += `    ${item.description}\n`;
      } else if (type === "decision") {
        msg += `  [decision] ${item.summary ?? "?"}\n`;
        if (item.rationale) msg += `    ${item.rationale}\n`;
      } else {
        msg += `  ${JSON.stringify(item)}\n`;
      }
    }
  }
  msg += "\nKeep these in mind while editing.";
  return msg;
}

/**
 * Format the blocking error message when error-severity violations exist on
 * the target file. Matching schemas render ahead of the violation list so
 * schema guidance reaches the agent even when the edit is blocked.
 */
export function formatBlockReason(
  path: string,
  schemas: unknown[],
  violations: unknown[] = [],
): string {
  let msg = formatSchemasSection(path, schemas);
  msg += `engrams check found error-severity violations in ${path}.\n`;
  msg += "Fix these before editing:\n\n";
  for (const v of violations) {
    const item = v as Record<string, unknown>;
    const sev = String(item.severity ?? "warn");
    if (sev !== "error") continue;
    msg += `  [error] ${item.pattern ?? "?"}: ${item.file ?? "?"}:${item.line ?? "?"}\n`;
  }
  return msg;
}

/**
 * Resolve a tool path to a workspace-relative path for `engrams advise`.
 * Paths that include a directory separator (or resolve inside `cwd` from an
 * absolute path) are returned even when the file does not exist on disk yet,
 * so `write` calls creating new files still receive schema and constraint
 * advice. Bare filenames that do not exist at the workspace root are
 * resolved via basename glob when unique, or skipped (`null`) when ambiguous.
 */
export function resolveFilePath(rawPath: string, cwd: string): string | null {
  const trimmed = rawPath.trim();
  if (!trimmed) return null;

  let candidate = trimmed;
  if (isAbsolute(candidate)) {
    const rel = relative(cwd, candidate);
    if (rel.startsWith("..") || isAbsolute(rel)) return null;
    candidate = rel;
  }
  while (candidate.startsWith("./")) {
    candidate = candidate.slice(2);
  }
  if (!candidate) return null;

  if (existsSync(join(cwd, candidate))) return candidate;
  if (candidate.includes("/")) return candidate;

  const base = basename(candidate);
  const glob = new Glob(`**/${base}`);
  const matches: string[] = [];
  for (const path of glob.scanSync({ cwd, onlyFiles: true })) {
    if (path.includes("target/") || path.includes(".git/") || path.includes("node_modules/")) {
      continue;
    }
    matches.push(path);
  }
  if (matches.length === 1) return matches[0];
  return null;
}

function findEngramsBinary(workspaceDir: string): string {
  const debug = join(workspaceDir, "target", "debug", "engrams");
  if (existsSync(debug)) return debug;
  const release = join(workspaceDir, "target", "release", "engrams");
  if (existsSync(release)) return release;
  return "engrams";
}

function resolveEngrams(cwd: string): { bin: string; db: string } | null {
  let dir = cwd;
  for (;;) {
    const db = join(dir, "engrams", "context.db");
    if (existsSync(db)) {
      return { bin: findEngramsBinary(dir), db };
    }
    const parent = resolve(dir, "..");
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

function extractPath(toolName: string, input: Record<string, unknown>): string | null {
  if (toolName === "write") {
    const p = String(input.path ?? "");
    return p || null;
  }
  if (toolName === "edit") {
    // Edit tool takes `input` containing hashline patch text starting
    // with [PATH#TAG] headers. Extract the first path.
    const text = String(input.input ?? "");
    const m = text.match(/^\[([^\]#]+)#/);
    return m ? m[1] : null;
  }
  return null;
}

async function runAdvise(
  bin: string,
  dbPath: string,
  targetPath: string,
  cwd: string,
): Promise<AdviseOutput | null> {
  try {
    const proc = Bun.spawn([bin, "--db", dbPath, "advise", targetPath], {
      cwd,
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    if (exitCode !== 0) return null;
    return parseAdviseOutput(stdout);
  } catch {
    return null;
  }
}

export default function engramsAdvisor(pi: ExtensionAPI): void {
  const advisedPaths = new Set<string>();

  pi.on("tool_call", async (event, ctx) => {
    const { toolName, input } = event;
    if (toolName !== "edit" && toolName !== "write") return;

    const rawPath = extractPath(toolName, input as Record<string, unknown>);
    if (!rawPath) return;
    const targetPath = resolveFilePath(rawPath, ctx.cwd);
    if (!targetPath) return;

    const key = `${ctx.cwd}::${targetPath}`;
    if (advisedPaths.has(key)) return;
    advisedPaths.add(key);

    const env = resolveEngrams(ctx.cwd);
    if (!env) return;

    const result = await runAdvise(env.bin, env.db, targetPath, ctx.cwd);
    if (!result) {
      if (ctx.hasUI) {
        ctx.ui.notify(
          `engrams advise failed for ${targetPath} — extension is a no-op until fixed`,
          "warn",
        );
      }
      return;
    }

    const { schemas, constraints, violations } = result;

    const hasErrors = violations.some((v) => {
      const item = v as Record<string, unknown>;
      return String(item.severity ?? "") === "error";
    });
    if (hasErrors) {
      advisedPaths.delete(key);
      return { block: true, reason: formatBlockReason(targetPath, schemas, violations) };
    }

    if (schemas.length > 0 || constraints.length > 0) {
      pi.sendMessage(formatSteer(targetPath, schemas, constraints), {
        deliverAs: "steer",
      });
    }
  });
}
