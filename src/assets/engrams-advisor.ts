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
import { existsSync, readFileSync, statSync } from "node:fs";
import { basename, dirname, isAbsolute, join, relative, resolve } from "node:path";
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
  if (schemas.length > 0) {
    msg += "\nKeep these in mind while editing, and cite any applied [schema] by name in your reply.";
  } else {
    msg += "\nKeep these in mind while editing.";
  }
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
 * Strip `read` line/query selectors (`path.rs:10-20`, `file.json?q=...`, `:raw`)
 * and ignore internal URI schemes (`skill://`, `rule://`, `http://`, etc.).
 */
export function stripReadSelector(rawPath: string): string | null {
  const trimmed = rawPath.trim();
  if (!trimmed || /^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(trimmed)) {
    return null;
  }
  const qIdx = trimmed.indexOf("?");
  const noQuery = qIdx >= 0 ? trimmed.slice(0, qIdx) : trimmed;
  const m = noQuery.match(/^(.+?):(?:\d|-\d|raw\b|img\b|conflicts\b)/);
  return (m ? m[1] : noQuery).trim() || null;
}

/**
 * Fold `<name>.worktrees/<branch>` path segments into `<name>`.
 */
export function foldWorktreeSegments(rawPath: string): string {
  let s = rawPath.trim();
  while (s.startsWith("./")) {
    s = s.slice(2);
  }
  if (s.length > 1 && s.endsWith("/")) {
    s = s.replace(/\/+$/, "");
  }
  if (!s.includes(".worktrees/")) {
    return s;
  }
  const leadingSlash = s.startsWith("/");
  const parts = s.split("/").filter(Boolean);
  const out: string[] = [];
  for (let i = 0; i < parts.length; i++) {
    const seg = parts[i];
    if (seg.endsWith(".worktrees") && seg.length > ".worktrees".length && i + 1 < parts.length) {
      out.push(seg.slice(0, -".worktrees".length));
      i += 1; // skip branch segment
    } else {
      out.push(seg);
    }
  }
  return (leadingSlash ? "/" : "") + out.join("/");
}

function resolveGitWorktreeMain(dir: string): string | null {
  const dotGit = join(dir, ".git");
  try {
    if (!existsSync(dotGit) || !statSync(dotGit).isFile()) return null;
    const content = readFileSync(dotGit, "utf8").trim();
    if (!content.startsWith("gitdir:")) return null;
    const rawGitdir = content.slice("gitdir:".length).trim();
    const absGitdir = isAbsolute(rawGitdir) ? rawGitdir : resolve(dir, rawGitdir);
    const parent = dirname(absGitdir);
    if (basename(parent) === "worktrees") {
      const mainRepo = dirname(dirname(parent));
      if (existsSync(mainRepo)) return mainRepo;
    }
  } catch {
    return null;
  }
  return null;
}

function foldAbsWorktreePath(absPath: string): string {
  let cur = absPath;
  while (true) {
    const mainRepo = resolveGitWorktreeMain(cur);
    if (mainRepo) {
      const rel = relative(cur, absPath);
      return foldWorktreeSegments(rel ? join(mainRepo, rel) : mainRepo);
    }
    const parent = dirname(cur);
    if (parent === cur) break;
    cur = parent;
  }
  return foldWorktreeSegments(absPath);
}

/**
 * Resolve a tool path to a canonical workspace-relative path for `engrams advise`.
 * Resolves relative paths against `cwd` first, folds git worktrees, and then
 * relativizes to `workspaceDir` (defaults to `cwd`).
 */
export function resolveFilePath(
  rawPath: string,
  cwd: string,
  workspaceDir: string = cwd,
): string | null {
  const stripped = stripReadSelector(rawPath);
  if (!stripped) return null;

  let candidate = stripped;
  while (candidate.startsWith("./")) {
    candidate = candidate.slice(2);
  }
  if (!candidate) return null;

  const absCandidate = isAbsolute(candidate) ? candidate : resolve(cwd, candidate);
  const foldedAbs = foldAbsWorktreePath(absCandidate);
  const foldedWorkspace = foldAbsWorktreePath(resolve(workspaceDir));
  const relToWorkspace = foldWorktreeSegments(relative(foldedWorkspace, foldedAbs));

  if (relToWorkspace.startsWith("..") || isAbsolute(relToWorkspace) || !relToWorkspace) {
    return null;
  }

  if (existsSync(absCandidate) || existsSync(join(foldedWorkspace, relToWorkspace))) {
    try {
      const st = existsSync(absCandidate)
        ? statSync(absCandidate)
        : statSync(join(foldedWorkspace, relToWorkspace));
      if (st.isDirectory()) return null;
    } catch {
      // ignore stat errors on race
    }
    return relToWorkspace;
  }
  if (candidate.includes("/") || relToWorkspace.includes("/")) {
    return relToWorkspace;
  }

  const base = basename(candidate);
  const glob = new Glob(`**/${base}`);
  const matches: string[] = [];
  for (const path of glob.scanSync({ cwd: foldedWorkspace, onlyFiles: true })) {
    if (path.includes("target/") || path.includes(".git/") || path.includes("node_modules/")) {
      continue;
    }
    matches.push(foldWorktreeSegments(path));
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

export function resolveEngrams(
  cwd: string,
): { bin: string; db: string; workspaceDir: string } | null {
  let dir = resolve(cwd);
  for (;;) {
    const db = join(dir, "engrams", "context.db");
    if (existsSync(db)) {
      return { bin: findEngramsBinary(dir), db, workspaceDir: dir };
    }
    const wtMain = resolveGitWorktreeMain(dir);
    const foldedDir = wtMain ?? foldWorktreeSegments(dir);
    if (foldedDir !== dir && existsSync(foldedDir)) {
      let wcur = foldedDir;
      for (;;) {
        const wdb = join(wcur, "engrams", "context.db");
        if (existsSync(wdb)) {
          return { bin: findEngramsBinary(wcur), db: wdb, workspaceDir: wcur };
        }
        const wparent = dirname(wcur);
        if (wparent === wcur) break;
        wcur = wparent;
      }
    }
    const parent = resolve(dir, "..");
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

function extractPath(toolName: string, input: Record<string, unknown>): string | null {
  if (toolName === "write" || toolName === "read") {
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

function formatAdvisedToast(
  targetPath: string,
  schemas: unknown[],
  constraints: unknown[],
): string {
  const schemaNames = schemas
    .map((s) => String((s as Record<string, unknown>).name ?? "?"))
    .filter(Boolean);
  const parts: string[] = [];
  if (schemaNames.length > 0) {
    parts.push(`schema [${schemaNames.join(", ")}]`);
  }
  if (constraints.length > 0) {
    parts.push(`${constraints.length} constraint${constraints.length === 1 ? "" : "s"}`);
  }
  return `engrams advised: ${parts.join(" + ")} (${basename(targetPath)})`;
}

async function runAdvise(
  bin: string,
  dbPath: string,
  targetPath: string,
  cwd: string,
  schemasOnly = false,
): Promise<AdviseOutput | null> {
  try {
    const args = [bin, "--db", dbPath, "advise"];
    if (schemasOnly) args.push("--schemas-only");
    args.push(targetPath);
    const proc = Bun.spawn(args, {
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
  const advisedReadSchemas = new Set<string>();

  pi.on("tool_call", async (event, ctx) => {
    const { toolName, input } = event;
    if (toolName !== "edit" && toolName !== "write" && toolName !== "read") return;

    const rawPath = extractPath(toolName, input as Record<string, unknown>);
    if (!rawPath) return;

    const env = resolveEngrams(ctx.cwd);
    if (!env) return;

    const targetPath = resolveFilePath(rawPath, ctx.cwd, env.workspaceDir);
    if (!targetPath) return;

    const strippedRaw = stripReadSelector(rawPath) ?? rawPath;
    const physicalCandidate = isAbsolute(strippedRaw)
      ? strippedRaw
      : resolve(ctx.cwd, strippedRaw);
    const cliPath = existsSync(physicalCandidate) ? physicalCandidate : targetPath;

    if (toolName === "read") {
      const result = await runAdvise(env.bin, env.db, cliPath, env.workspaceDir, true);
      if (!result) return;
      const unseenSchemas = result.schemas.filter((s) => {
        const item = s as Record<string, unknown>;
        const key = String(item.id ?? item.name ?? "");
        if (!key || advisedReadSchemas.has(key)) return false;
        advisedReadSchemas.add(key);
        return true;
      });
      if (unseenSchemas.length > 0) {
        if (ctx.hasUI) {
          ctx.ui.notify(formatAdvisedToast(targetPath, unseenSchemas, []), "info");
        }
        pi.sendMessage(formatSteer(targetPath, unseenSchemas, []), {
          deliverAs: "steer",
        });
      }
      return;
    }

    const key = `${env.workspaceDir}::${targetPath}`;
    if (advisedPaths.has(key)) return;
    advisedPaths.add(key);

    const result = await runAdvise(env.bin, env.db, cliPath, env.workspaceDir, false);
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
    for (const s of schemas) {
      const item = s as Record<string, unknown>;
      const skey = String(item.id ?? item.name ?? "");
      if (skey) advisedReadSchemas.add(skey);
    }

    const hasErrors = violations.some((v) => {
      const item = v as Record<string, unknown>;
      return String(item.severity ?? "") === "error";
    });
    if (hasErrors) {
      advisedPaths.delete(key);
      return { block: true, reason: formatBlockReason(targetPath, schemas, violations) };
    }

    if (schemas.length > 0 || constraints.length > 0) {
      if (ctx.hasUI) {
        ctx.ui.notify(formatAdvisedToast(targetPath, schemas, constraints), "info");
      }
      pi.sendMessage(formatSteer(targetPath, schemas, constraints), {
        deliverAs: "steer",
      });
    }
  });
}
