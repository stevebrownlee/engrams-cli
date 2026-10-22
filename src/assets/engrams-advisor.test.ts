import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";
import engramsAdvisor, {
  formatBlockReason,
  formatSteer,
  parseAdviseOutput,
  resolveFilePath,
} from "./engrams-advisor";

interface RecordedMessage {
  content: string;
  options: Record<string, unknown>;
}

type ToolCallHandler = (
  event: { toolName: string; input: Record<string, unknown> },
  ctx: {
    cwd: string;
    hasUI: boolean;
    ui: { notify: (message: string, level: string) => void };
  },
) => Promise<{ block: boolean; reason: string } | undefined>;

function createMockExtensionApi(): {
  pi: ExtensionAPI;
  messages: RecordedMessage[];
  invokeToolCall: ToolCallHandler;
} {
  const messages: RecordedMessage[] = [];
  let registeredHandler: ToolCallHandler | null = null;

  const pi = {
    on(event: string, handler: ToolCallHandler) {
      if (event === "tool_call") {
        registeredHandler = handler;
      }
    },
    sendMessage(content: string, options: Record<string, unknown>) {
      messages.push({ content, options });
    },
  } as unknown as ExtensionAPI;

  engramsAdvisor(pi);

  return {
    pi,
    messages,
    async invokeToolCall(event, ctx) {
      if (!registeredHandler) {
        throw new Error("tool_call handler was not registered");
      }
      return registeredHandler(event, ctx);
    },
  };
}

interface ManuscriptBookFixture {
  provenance: {
    protagonist_claim: string;
    protagonist_instruction: string;
  };
  facts: {
    decisions: Array<{
      id: number;
      timestamp: string;
      summary: string;
      tags: string;
    }>;
    patterns: Array<{
      id: number;
      timestamp: string;
      name: string;
      description: string | null;
      tags: string | null;
      check_kind: string | null;
      check_expr: string | null;
      severity: string | null;
    }>;
    progress: Array<{
      id: number;
      timestamp: string;
      status: string;
      description: string;
    }>;
    custom: Array<{
      id: number;
      timestamp: string;
      category: string;
      key: string;
      value: string;
    }>;
    code: Array<{
      id: number;
      first_seen: string;
      last_seen: string;
      path: string;
      symbol: string;
    }>;
    anchors: Array<{
      item_type: string;
      item_id: number;
      path: string;
      timestamp: string;
    }>;
    links: Array<{
      source_type: string;
      source_id: string;
      target_type: string;
      target_id: string;
      rel: string;
      timestamp: string;
    }>;
  };
}

function runEngramsSync(bin: string, dbPath: string, args: string[], cwd: string): unknown {
  const res = Bun.spawnSync([bin, "--db", dbPath, ...args], {
    cwd,
    env: { ...process.env, ENGRAMS_NO_UPDATE_CHECK: "1" },
    stdout: "pipe",
    stderr: "pipe",
  });
  const stdout = new TextDecoder().decode(res.stdout);
  const stderr = new TextDecoder().decode(res.stderr);
  if (res.exitCode !== 0) {
    throw new Error(`engrams ${args.join(" ")} failed (${res.exitCode}): ${stderr || stdout}`);
  }
  return JSON.parse(stdout);
}

function setupManuscriptWorkspace(): {
  workspaceDir: string;
  dbPath: string;
  binPath: string;
  fixture: ManuscriptBookFixture;
  cleanup: () => void;
} {
  const repoRoot = resolve(import.meta.dir, "../..");
  const builtBinary = join(repoRoot, "target", "debug", "engrams");
  if (!existsSync(builtBinary)) {
    throw new Error(`Expected built debug binary at ${builtBinary}; run cargo build first`);
  }

  const workspaceDir = mkdtempSync(join(tmpdir(), "engrams-advisor-book-"));
  const targetDebugDir = join(workspaceDir, "target", "debug");
  mkdirSync(targetDebugDir, { recursive: true });
  const binPath = join(targetDebugDir, "engrams");
  symlinkSync(builtBinary, binPath);

  const dbPath = join(workspaceDir, "engrams", "context.db");
  runEngramsSync(binPath, dbPath, ["init"], workspaceDir);

  const fixturePath = join(repoRoot, "tests", "fixtures", "manuscript-book.json");
  const fixture = JSON.parse(readFileSync(fixturePath, "utf8")) as ManuscriptBookFixture;

  const db = new Database(dbPath);
  const seedTx = db.transaction(() => {
    const insertDecision = db.prepare(
      "INSERT INTO decisions (id, uuid, timestamp, summary, tags, status, archived) VALUES (?1, ?2, ?3, ?4, ?5, 'active', 0)",
    );
    for (const d of fixture.facts.decisions) {
      insertDecision.run(d.id, `u-dec-${d.id}`, d.timestamp, d.summary, d.tags);
    }

    const insertPattern = db.prepare(
      "INSERT INTO system_patterns (id, uuid, timestamp, name, description, tags, check_kind, check_expr, severity, archived) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
    );
    for (const p of fixture.facts.patterns) {
      insertPattern.run(
        p.id,
        `u-pat-${p.id}`,
        p.timestamp,
        p.name,
        p.description,
        p.tags,
        p.check_kind,
        p.check_expr,
        p.severity ?? "warn",
      );
    }

    const insertProgress = db.prepare(
      "INSERT INTO progress_entries (id, timestamp, status, description) VALUES (?1, ?2, ?3, ?4)",
    );
    for (const g of fixture.facts.progress) {
      insertProgress.run(g.id, g.timestamp, g.status || "Done", g.description);
    }

    const insertCustom = db.prepare(
      "INSERT INTO custom_data (id, timestamp, category, key, value) VALUES (?1, ?2, ?3, ?4, ?5)",
    );
    for (const c of fixture.facts.custom) {
      insertCustom.run(c.id, c.timestamp, c.category, c.key, c.value);
    }

    const insertCode = db.prepare(
      "INSERT INTO code_nodes (id, kind, path, symbol, first_seen, last_seen) VALUES (?1, 'file', ?2, ?3, ?4, ?5)",
    );
    for (const code of fixture.facts.code) {
      insertCode.run(code.id, code.path, code.symbol, code.first_seen, code.last_seen);
    }

    const insertAnchor = db.prepare(
      "INSERT INTO item_anchors (item_type, item_id, path, timestamp) VALUES (?1, ?2, ?3, ?4)",
    );
    for (const a of fixture.facts.anchors) {
      insertAnchor.run(a.item_type, a.item_id, a.path, a.timestamp);
    }

    const insertLink = db.prepare(
      "INSERT INTO context_links (source_item_type, source_item_id, target_item_type, target_item_id, relationship_type, timestamp, origin) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'manual')",
    );
    for (const l of fixture.facts.links) {
      insertLink.run(
        l.source_type,
        l.source_id,
        l.target_type,
        l.target_id,
        l.rel,
        l.timestamp,
      );
    }
  });
  seedTx();
  db.close();

  runEngramsSync(
    binPath,
    dbPath,
    [
      "schema",
      "propose",
      "--name",
      "mara-broken-leader",
      "--statement",
      fixture.provenance.protagonist_claim,
      "--summary",
      fixture.provenance.protagonist_instruction,
      "--members",
      "decision:1,decision:2,decision:3,decision:4,decision:5,decision:6,progress_entry:1,progress_entry:2",
      "--scope-tags",
      "mara",
      "--scope-paths",
      "manuscript",
    ],
    workspaceDir,
  );
  runEngramsSync(binPath, dbPath, ["schema", "confirm", "proposal:1"], workspaceDir);

  return {
    workspaceDir,
    dbPath,
    binPath,
    fixture,
    cleanup() {
      rmSync(workspaceDir, { recursive: true, force: true });
    },
  };
}

describe("engrams-advisor pure helpers", () => {
  test("parseAdviseOutput retains schemas alongside constraints and violations", () => {
    const raw = {
      schemas: [
        {
          id: 1,
          name: "mara-broken-leader",
          statement:
            "Mara believes she is broken; that hurt carries with her as she grows into a leader",
          summary:
            "When writing Mara under pressure: she doubts herself aloud and credits others, even as her actions grow stronger",
          scope: { tags: ["mara"], paths: ["manuscript"] },
          matched_by: { via: "scope_path", value: "manuscript" },
        },
      ],
      constraints: [{ type: "decision", id: 1, summary: "Mara cannot channel the spire-seal" }],
      violations: [
        {
          pattern: "No Modern Idioms in Dialogue",
          file: "manuscript/ch06.md",
          line: 1,
          severity: "error",
        },
      ],
    };

    const parsed = parseAdviseOutput(JSON.stringify(raw));
    expect(parsed.schemas).toEqual(raw.schemas);
    expect(parsed.constraints).toEqual(raw.constraints);
    expect(parsed.violations).toEqual(raw.violations);

    const empty = parseAdviseOutput({});
    expect(empty).toEqual({ schemas: [], constraints: [], violations: [] });
  });

  test("formatSteer puts each schema's claim, instruction, scope, and match reason before any constraint", () => {
    const schemas = [
      {
        id: 1,
        name: "mara-broken-leader",
        statement:
          "Mara believes she is broken; that hurt carries with her as she grows into a leader",
        summary:
          "When writing Mara under pressure: she doubts herself aloud and credits others, even as her actions grow stronger",
        scope: { tags: ["mara"], paths: ["manuscript"] },
        matched_by: { via: "scope_path", value: "manuscript" },
      },
    ];
    const constraints = [
      {
        type: "pattern",
        id: 2,
        name: "No Modern Idioms in Dialogue",
        check_kind: "regex",
        check_expr: "\\b(okay|OK|roger that)\\b",
        severity: "error",
        description:
          "Avoid anachronistic military slang such as 'okay' or 'Roger' in manuscript chapters.",
      },
      {
        type: "decision",
        id: 21,
        summary:
          "Keep battle scenes in tight third-person limited on the chapter's point-of-view officer",
      },
    ];

    const steer = formatSteer("manuscript/ch19.md", schemas, constraints);
    expect(steer).toBe(
      [
        "engrams schemas for manuscript/ch19.md:",
        "  [schema] mara-broken-leader",
        "    Mara believes she is broken; that hurt carries with her as she grows into a leader",
        "    When writing Mara under pressure: she doubts herself aloud and credits others, even as her actions grow stronger",
        "    scope: paths manuscript; tags mara (matched: scope path manuscript)",
        "engrams constraints for manuscript/ch19.md:",
        "  [error] No Modern Idioms in Dialogue — regex: /\\b(okay|OK|roger that)\\b/",
        "    Avoid anachronistic military slang such as 'okay' or 'Roger' in manuscript chapters.",
        "  [decision] Keep battle scenes in tight third-person limited on the chapter's point-of-view officer",
        "",
        "Keep these in mind while editing.",
      ].join("\n"),
    );

    const schemaOnly = formatSteer("manuscript/ch19.md", schemas, []);
    expect(schemaOnly).toContain("engrams schemas for manuscript/ch19.md:");
    expect(schemaOnly).not.toContain("engrams constraints for manuscript/ch19.md:");

    const preV16Schema = [
      {
        id: 2,
        name: "performance-rules",
        statement: null,
        summary: "Fires when editing Rust hot paths: follow the four allocation rules",
        scope: { tags: [], paths: ["src/ops"] },
        matched_by: { via: "member_anchor", value: "src/ops/advise.rs" },
      },
    ];
    const preV16Steer = formatSteer("src/ops/advise.rs", preV16Schema, []);
    expect(preV16Steer).toBe(
      [
        "engrams schemas for src/ops/advise.rs:",
        "  [schema] performance-rules",
        "    Fires when editing Rust hot paths: follow the four allocation rules",
        "    scope: paths src/ops (matched: member anchor src/ops/advise.rs)",
        "",
        "Keep these in mind while editing.",
      ].join("\n"),
    );
  });

  test("formatBlockReason puts schema entries before error-severity violations", () => {
    const schemas = [
      {
        id: 1,
        name: "mara-broken-leader",
        statement:
          "Mara believes she is broken; that hurt carries with her as she grows into a leader",
        summary:
          "When writing Mara under pressure: she doubts herself aloud and credits others, even as her actions grow stronger",
        scope: { tags: ["mara"], paths: ["manuscript"] },
        matched_by: { via: "scope_path", value: "manuscript" },
      },
    ];
    const violations = [
      {
        pattern: "No Modern Idioms in Dialogue",
        file: "manuscript/ch06.md",
        line: 1,
        severity: "error",
      },
      {
        pattern: "Said-Bookism Restraint",
        file: "manuscript/ch06.md",
        line: 3,
        severity: "warn",
      },
    ];

    const reason = formatBlockReason("manuscript/ch06.md", schemas, violations);
    expect(reason).toBe(
      [
        "engrams schemas for manuscript/ch06.md:",
        "  [schema] mara-broken-leader",
        "    Mara believes she is broken; that hurt carries with her as she grows into a leader",
        "    When writing Mara under pressure: she doubts herself aloud and credits others, even as her actions grow stronger",
        "    scope: paths manuscript; tags mara (matched: scope path manuscript)",
        "engrams check found error-severity violations in manuscript/ch06.md.",
        "Fix these before editing:",
        "",
        "  [error] No Modern Idioms in Dialogue: manuscript/ch06.md:1",
        "",
      ].join("\n"),
    );
  });

  test("resolveFilePath resolves new relative paths and skips unknown bare filenames", () => {
    const dir = mkdtempSync(join(tmpdir(), "engrams-advisor-path-"));
    try {
      expect(resolveFilePath("manuscript/ch19.md", dir)).toBe("manuscript/ch19.md");
      expect(resolveFilePath(join(dir, "manuscript/ch19.md"), dir)).toBe("manuscript/ch19.md");
      expect(resolveFilePath("ch19.md", dir)).toBeNull();
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("engrams-advisor tool_call handler", () => {
  test("AC-5: write of a new manuscript/ch19.md sends the confirmed protagonist schema steer", async () => {
    const ws = setupManuscriptWorkspace();
    try {
      const { messages, invokeToolCall } = createMockExtensionApi();
      const ctx = {
        cwd: ws.workspaceDir,
        hasUI: false,
        ui: { notify: () => {} },
      };

      expect(existsSync(join(ws.workspaceDir, "manuscript", "ch19.md"))).toBe(false);

      const res = await invokeToolCall(
        {
          toolName: "write",
          input: {
            path: "manuscript/ch19.md",
            content: "# Chapter 19\nMara stood on the northern watchtower.\n",
          },
        },
        ctx,
      );

      expect(res).toBeUndefined();
      expect(messages).toHaveLength(1);
      expect(messages[0].options).toEqual({ deliverAs: "steer" });
      expect(messages[0].content).toBe(
        [
          "engrams schemas for manuscript/ch19.md:",
          "  [schema] mara-broken-leader",
          `    ${ws.fixture.provenance.protagonist_claim}`,
          `    ${ws.fixture.provenance.protagonist_instruction}`,
          "    scope: paths manuscript; tags mara (matched: scope path manuscript)",
          "",
          "Keep these in mind while editing.",
        ].join("\n"),
      );

      // A second write to the same file in the same session does not re-send.
      await invokeToolCall(
        {
          toolName: "write",
          input: { path: "manuscript/ch19.md", content: "# Chapter 19\nUpdated.\n" },
        },
        ctx,
      );
      expect(messages).toHaveLength(1);
    } finally {
      ws.cleanup();
    }
  });

  test("AC-12: edit of an existing chapter with an error-severity violation blocks with the schema first, then steers on retry", async () => {
    const ws = setupManuscriptWorkspace();
    try {
      mkdirSync(join(ws.workspaceDir, "manuscript"), { recursive: true });
      const ch06Path = join(ws.workspaceDir, "manuscript", "ch06.md");
      writeFileSync(ch06Path, 'The lieutenant muttered, "roger that, captain."\n');

      const { messages, invokeToolCall } = createMockExtensionApi();
      const ctx = {
        cwd: ws.workspaceDir,
        hasUI: false,
        ui: { notify: () => {} },
      };

      const blocked = await invokeToolCall(
        {
          toolName: "edit",
          input: {
            input: "[manuscript/ch06.md#A1B2]\nPUT 1.=1:\n+\"Roger that,\" he repeated.",
          },
        },
        ctx,
      );

      expect(messages).toHaveLength(0);
      expect(blocked).toBeDefined();
      expect(blocked?.block).toBe(true);
      expect(blocked?.reason).toBe(
        [
          "engrams schemas for manuscript/ch06.md:",
          "  [schema] mara-broken-leader",
          `    ${ws.fixture.provenance.protagonist_claim}`,
          `    ${ws.fixture.provenance.protagonist_instruction}`,
          "    scope: paths manuscript; tags mara (matched: scope path manuscript)",
          "engrams check found error-severity violations in manuscript/ch06.md.",
          "Fix these before editing:",
          "",
          "  [error] No Modern Idioms in Dialogue: manuscript/ch06.md:1",
          "",
        ].join("\n"),
      );

      // Blocked path stays un-advised: once the violation is fixed, the retry
      // delivers the normal steer message with the schema ahead of constraints.
      writeFileSync(ch06Path, '"Understood, captain," Toren said.\n');

      const retried = await invokeToolCall(
        {
          toolName: "edit",
          input: {
            input: "[manuscript/ch06.md#A1B2]\nPUT 1.=1:\n+\"Understood,\" he repeated.",
          },
        },
        ctx,
      );

      expect(retried).toBeUndefined();
      expect(messages).toHaveLength(1);
      expect(messages[0].options).toEqual({ deliverAs: "steer" });
      const schemaIdx = messages[0].content.indexOf("engrams schemas for manuscript/ch06.md:");
      const constraintIdx = messages[0].content.indexOf(
        "engrams constraints for manuscript/ch06.md:",
      );
      expect(schemaIdx).toBe(0);
      expect(constraintIdx).toBeGreaterThan(schemaIdx);
      expect(messages[0].content).toContain("No Modern Idioms in Dialogue");
    } finally {
      ws.cleanup();
    }
  });
});
