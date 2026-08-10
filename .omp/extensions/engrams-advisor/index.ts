/**
 * engrams pre-edit advisor — omp extension.
 *
 * Intercepts `edit` and `write` tool calls. On the first edit to each file per
 * session, runs `engrams advise <path>` and delivers constraints as a
 * non-blocking steer message. Blocks only on error-severity violations.
 *
 * Self-disables outside engrams workspaces (no engrams/context.db found in
 * ancestor directories).
 *
 * Installed by: engrams install --harness omp --hooks
 * Or manually: cp into .omp/extensions/engrams-advisor/index.ts
 */
import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";

export default function engramsAdvisor(pi: ExtensionAPI): void {
  const advisedPaths = new Set<string>();

  // ── Binary resolution ──────────────────────────────────────────────
  // Walk up from cwd to find a workspace containing engrams/context.db.
  // If found, resolve the engrams binary (debug → release → PATH).
  // Returns null if not in an engrams workspace — hook becomes a no-op.
  function resolveEngrams(cwd: string): { bin: string; db: string } | null {
    let dir = cwd;
    for (;;) {
      const db = join(dir, "engrams", "context.db");
      if (existsSync(db)) {
        const debug = join(dir, "target", "debug", "engrams");
        const release = join(dir, "target", "release", "engrams");
        const bin = existsSync(debug)
          ? debug
          : existsSync(release)
            ? release
            : "engrams"; // fall back to PATH
        return { bin, db };
      }
      const parent = resolve(dir, "..");
      if (parent === dir) break;
      dir = parent;
    }
    return null;
  }

  // ── Path extraction ────────────────────────────────────────────────
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

  // ── Run engrams advise ─────────────────────────────────────────────
  async function runAdvise(
    bin: string,
    dbPath: string,
    targetPath: string,
    cwd: string,
  ): Promise<{ constraints: unknown[]; violations: unknown[] } | null> {
    try {
      const proc = Bun.spawn([bin, "--db", dbPath, "advise", targetPath], {
        cwd,
        stdout: "pipe",
        stderr: "pipe",
      });
      const stdout = await new Response(proc.stdout).text();
      const exitCode = await proc.exited;
      if (exitCode !== 0) return null;
      const data = JSON.parse(stdout);
      return {
        constraints: Array.isArray(data.constraints) ? data.constraints : [],
        violations: Array.isArray(data.violations) ? data.violations : [],
      };
    } catch {
      return null;
    }
  }

  // ── Format advisory text ───────────────────────────────────────────
  function formatSteer(path: string, constraints: unknown[]): string {
    let msg = `engrams constraints for ${path}:\n`;
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
    msg += "\nKeep these in mind while editing.";
    return msg;
  }

  function formatBlockReason(
    path: string,
    violations: unknown[],
  ): string {
    let msg = `engrams check found error-severity violations in ${path}.\n`;
    msg += "Fix these before editing:\n\n";
    for (const v of violations) {
      const item = v as Record<string, unknown>;
      const sev = String(item.severity ?? "warn");
      if (sev !== "error") continue;
      msg += `  [error] ${item.pattern ?? "?"}: ${item.file ?? "?"}:${item.line ?? "?"}\n`;
    }
    return msg;
  }

  // ── Hook: intercept edit/write ─────────────────────────────────────
  pi.on("tool_call", async (event, ctx) => {
    const { toolName, input } = event;
    if (toolName !== "edit" && toolName !== "write") return;

    const targetPath = extractPath(toolName, input as Record<string, unknown>);
    if (!targetPath) return;

    // Once per file per session
    const key = `${ctx.cwd}::${targetPath}`;
    if (advisedPaths.has(key)) return;
    advisedPaths.add(key);

    // Resolve engrams workspace + binary
    const env = resolveEngrams(ctx.cwd);
    if (!env) return; // Not an engrams workspace — silent no-op

    const result = await runAdvise(env.bin, env.db, targetPath, ctx.cwd);
    if (!result) {
      // Binary found but execution failed — visible warning, not silent
      if (ctx.hasUI) {
        ctx.ui.notify(
          `engrams advise failed for ${targetPath} — extension is a no-op until fixed`,
          "warn",
        );
      }
      return;
    }

    const { constraints, violations } = result;

    // Block on error-severity violations (mechanical enforcement)
    const hasErrors = violations.some((v) => {
      const item = v as Record<string, unknown>;
      return String(item.severity ?? "") === "error";
    });
    if (hasErrors) {
      advisedPaths.delete(key);
      return { block: true, reason: formatBlockReason(targetPath, violations) };
    }

    // Non-blocking steer for constraints (advisory, edit still lands)
    if (constraints.length > 0) {
      pi.sendMessage(formatSteer(targetPath, constraints), {
        deliverAs: "steer",
      });
    }

    // No constraints, no error violations — edit proceeds normally
  });
}
