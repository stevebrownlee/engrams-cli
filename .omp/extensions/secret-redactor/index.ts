/**
 * Secret Redactor — omp extension
 *
 * Two layers of protection:
 *
 * 1. tool_call: BLOCKS reads of known secret files (.env, *.pem, *.key,
 *    id_rsa, credentials.json, *.pfx) so they never reach the LLM.
 *
 * 2. tool_result: SCANS all text output from every tool for leaked secrets
 *    (API keys, DB connection strings, private keys, JWTs, bearer tokens)
 *    and replaces them with [REDACTED:<type>] before the LLM sees them.
 *
 * Install: cp -r . ~/.omp/agent/extensions/secret-redactor
 * Or:      omp --extension ./secret-redactor
 */

import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";

// ---------------------------------------------------------------------------
// Secret patterns
// ---------------------------------------------------------------------------

interface SecretPattern {
  name: string;
  regex: RegExp;
}

const SECRET_PATTERNS: SecretPattern[] = [
  // Cloud provider keys
  { name: "aws-access-key", regex: /\bAKIA[0-9A-Z]{16}\b/g },
  { name: "aws-secret", regex: /\baws_secret_access_key\s*[=:]\s*["']?[A-Za-z0-9/+=]{40}["']?/gi },
  { name: "google-api-key", regex: /\bAIza[0-9A-Za-z\-_]{35}\b/g },

  // GitHub tokens
  { name: "github-pat", regex: /\bghp_[A-Za-z0-9]{36,}\b/g },
  { name: "github-oauth", regex: /\bgho_[A-Za-z0-9]{36,}\b/g },
  { name: "github-app", regex: /\b(ghs_|ghu_|ghr_)[A-Za-z0-9]{36,}\b/g },

  // LLM provider keys
  { name: "openai-key", regex: /\bsk-[A-Za-z0-9]{20,}\b/g },
  { name: "anthropic-key", regex: /\bsk-ant-[A-Za-z0-9\-_]{50,}\b/g },

  // Database connection strings (capture the full URI with credentials)
  {
    name: "postgres-conn",
    regex: /\bpostgres(?:ql)?:\/\/[^:\s]+:[^@\s]+@[^\s"'<>]+/gi,
  },
  {
    name: "mysql-conn",
    regex: /\bmysql:\/\/[^:\s]+:[^@\s]+@[^\s"'<>]+/gi,
  },
  {
    name: "mongodb-conn",
    regex: /\bmongodb(?:\+srv)?:\/\/[^:\s]+:[^@\s]+@[^\s"'<>]+/gi,
  },
  {
    name: "redis-conn",
    regex: /\bredis:\/\/[^:\s]+:[^@\s]+@[^\s"'<>]+/gi,
  },
  // Generic DB connection with password= param
  {
    name: "db-password-param",
    regex: /\b(?:host|port|dbname|user|password)=[^\s;&]+(?:[;&](?:host|port|dbname|user|password)=[^\s;&]+)*/gi,
  },

  // JWT tokens (three base64 segments separated by dots)
  {
    name: "jwt",
    regex: /\beyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b/g,
  },

  // Bearer / Authorization headers
  {
    name: "bearer-token",
    regex: /\b(?:Bearer|Authorization)\s*[:=]?\s*[A-Za-z0-9\-._~+\/=]{20,}/gi,
  },

  // Private key blocks (entire PEM block)
  {
    name: "private-key",
    regex: /-----BEGIN (?:RSA |EC |DSA |OPENSSH |PGP |ENCRYPTED )?PRIVATE KEY-----[\s\S]*?-----END (?:RSA |EC |DSA |OPENSSH |PGP |ENCRYPTED )?PRIVATE KEY-----/g,
  },

  // Slack tokens
  { name: "slack-bot", regex: /\bxoxb-[0-9A-Za-z-]+/g },
  { name: "slack-user", regex: /\bxox[ps]-[0-9A-Za-z-]+/g },

  // Stripe
  { name: "stripe-key", regex: /\b(?:sk|pk|rk)_(?:live|test)_[0-9A-Za-z]{24,}\b/g },

  // Generic ENV-style secret assignments for known secret key names
  {
    name: "env-secret",
    regex: /\b(API_KEY|API_SECRET|SECRET_KEY|ACCESS_TOKEN|REFRESH_TOKEN|CLIENT_SECRET|PRIVATE_KEY|ENCRYPTION_KEY|JWT_SECRET|DATABASE_URL|DB_PASSWORD|AWS_SECRET_ACCESS_KEY|STRIPE_SECRET_KEY|SENDGRID_API_KEY|MAILGUN_API_KEY|TWILIO_AUTH_TOKEN|FIREBASE_API_KEY|GOOGLE_CLIENT_SECRET)\s*[:=]\s*["']?[^\s"'<>\n]{8,}["']?/gi,
  },
];

// Files that should NEVER be read by the LLM
const BLOCKED_FILE_PATTERNS = [
  /\.env(?:\.[a-z]+)?$/i,
  /\.env$/i,
  /\/\.env\./i,
  /\.(pem|key|pfx|p12|jks|keystore)$/i,
  /\/id_rsa(?:\.pub)?$/,
  /\/id_ecdsa$/,
  /\/id_ed25519$/,
  /\/credentials\.json$/i,
  /\/service-account.*\.json$/i,
  /\/secrets?\//i,
  /\/\.npmrc$/,
  /\/\.pypirc$/,
  /\/\.dockercfg$/,
  /\/\.git-credentials$/,
  /\/\.netrc$/,
];

function isBlockedFile(path: string): boolean {
  return BLOCKED_FILE_PATTERNS.some((p) => p.test(path));
}

function redactText(text: string): { text: string; changed: boolean } {
  let result = text;
  let changed = false;

  for (const { name, regex } of SECRET_PATTERNS) {
    const replacement = `[REDACTED:${name}]`;
    const next = result.replace(regex, replacement);
    if (next !== result) {
      changed = true;
      result = next;
    }
  }

  return { text: result, changed };
}

export default function secretRedactor(pi: ExtensionAPI): void {
  pi.setLabel("Secret Redactor");

  // --- Layer 1: Block reads of known secret files ---

  pi.on("tool_call", async (event) => {
    if (event.toolName !== "read") return;

    const path = String(event.input.path ?? "");
    if (!path) return;

    if (isBlockedFile(path)) {
      return {
        block: true,
        reason: `File '${path}' matches a blocked secret/credential pattern. Its contents are not available to the LLM. If you need a value from this file, ask the user to provide it directly.`,
      };
    }
  });

  // --- Layer 2: Redact secrets in ALL tool output ---

  pi.on("tool_result", async (event) => {
    if (event.isError) return;

    // Skip tools that don't return user-content text
    const contentTools = ["read", "bash", "grep", "eval", "write", "ast_edit"];
    if (!contentTools.includes(event.toolName)) return;

    let anyChanged = false;
    const redacted = event.content.map((chunk) => {
      if (chunk.type !== "text") return chunk;

      const { text, changed } = redactText(chunk.text);
      if (changed) anyChanged = true;
      return { ...chunk, text };
    });

    if (anyChanged) return { content: redacted };
  });
}
