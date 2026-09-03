# Product Requirements Document (PRD): ROUTINGALL (v1.6 — Local Proxy, Hardened)

## 1. Document Overview
- **Product Name:** ROUTINGALL
- **Product Type:** Lightweight Local Developer Proxy & Master-Key Gateway
- **Form Factor:** Standalone Desktop Application (Tauri) with System Tray Integration
- **Status:** Specification & Architecture Phase

> **Changes from v1.2:** defines tool-calling/payload translation scope per provider, adds a health/status endpoint, defines key cleanup on uninstall, adds key-format validation on entry, adds single-instance locking, port-conflict handling, provider call timeouts, and a defined error body shape. CORS is called out as a known open decision, not resolved here. Full changelog in §10.

---

## 2. Executive Summary & Vision
ROUTINGALL is a zero-cloud, local developer utility that solves fragmented API key management and environment setup overhead. It runs a lightweight local proxy at `http://127.0.0.1:8081/v1`, wrapping multiple AI providers (Groq, Google AI Studio, OpenRouter) behind a single Master Virtual Key and a unified OpenAI-compatible format.

---

## 3. Core Goals & Non-Goals

**Goals**
- Single endpoint: point any OpenAI SDK client to `http://127.0.0.1:8081/v1`.
- Master Key authentication for all local traffic.
- Provider keys stored exclusively in the OS keychain.
- Zero custom SDKs — standard OpenAI-format compatibility, including tool/function calling within defined scope (§6).

**Non-Goals**
- No cloud hosting/backend. No databases, no user tracking.
- No automatic failover between providers.
- No key pooling / free-tier exploitation.
- No Gemini multimodal (`inlineData`) translation in v1 — Gemini `image_url` requests are blocked with a `400` until this conversion is built (explicitly deferred — see §6.3). Groq and OpenRouter multimodal requests are **not** blocked; both accept `image_url` natively and are passed through as-is (see §6.1, §6.3).

---

## 4. System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                   LOCAL APPLICATION CODE                    │
│  Client SDK (OpenAI format) → base_url: 127.0.0.1:8081/v1   │
└──────────────────────────────┬──────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────┐
│                     ROUTINGALL PROXY                          │
│              (Running locally on 127.0.0.1:8081)              │
│                                                                │
│  [1. Single-Instance Lock]  → Refuses 2nd launch                │
│  [2. Master Key Validation] → Verifies rg-master-key            │
│  [3. Model Prefix Parsing]  → First-slash split: provider/model │
│  [4. Key Resolution]        → OS keychain lookup                │
│  [5. Payload Translation]   → Tool calls, params, per §6         │
│  [6. Payload Forwarding]    → Routes to provider, with timeout   │
│  [7. Response Translation]  → Streaming + error body normalize   │
└──────────────────────────────┬──────────────────────────────┘
                                │
          ┌─────────────────────┼─────────────────────┐
          ▼                     ▼                     ▼
    [ Groq API ]       [ Google AI Studio ]      [ OpenRouter ]
```

### Technical Specs

| Parameter | Specification |
|---|---|
| Default Address | `http://127.0.0.1:8081/v1` (loopback only) |
| Health Endpoint | `http://127.0.0.1:8081/v1/health` (see §7) |
| Authentication Scheme | Bearer Token (`Authorization: Bearer rg-master-key-...`) |
| Supported Providers | Groq, Google AI Studio (Gemini), OpenRouter |
| State Management | OS-native credential store only |
| Runtime Environment | Tauri (Rust backend + lightweight web frontend) |
| Provider Call Timeout | 60s default, configurable in GUI (§9.4) |
| CORS | **Open decision, not resolved in this spec** — if browser-based clients are ever in scope, the proxy needs `Access-Control-Allow-Origin` handling; server-side/script clients (the primary target) are unaffected. Flagged for a future revision. |

---

## 5. Functional Requirements (Core)

### FR-1: Desktop App & System Tray Shell
- Launches as a lightweight desktop process; minimizes to tray.
- Minimal GUI for key management, Master Key generation, and status (§7).

### FR-1a: Window Close vs. Quit Semantics (new — resolves ambiguity between FR-1 and FR-2)
Two distinct, clearly-labeled actions, so the proxy's running state is never ambiguous to the developer:

| Action | Effect |
|---|---|
| Clicking the window's close (X) button | Window hides to tray only. The background process and the proxy listener on `127.0.0.1:8081` **keep running**. This is the default/expected state during a dev session. |
| Tray icon → **"Quit ROUTINGALL"** | The only action that terminates the listener and exits the process entirely (this is what FR-2's "cleanly terminates on app exit" refers to). |

- The tray icon shows a visual running/stopped indicator (e.g., colored dot) so it's never unclear whether the proxy is alive in the background.
- Rationale: without this split, a developer clicking X expecting to fully quit could either (a) unknowingly leave the proxy running, or (b) unknowingly kill it mid-session and get silent connection-refused errors from their scripts. Making Quit an explicit, separate action from Close removes that ambiguity.

### FR-2: Local Proxy Server
- Binds to `127.0.0.1:8081` only; non-loopback binding is a startup failure.
- **Port conflict handling:** if `8081` is already bound by another process, the app does **not** silently pick a different port. It shows a GUI error ("Port 8081 is in use by another process") and offers a manual override to configure an alternate port, persisted in local config. No auto-fallback, since the base URL must stay predictable for client code.
- **Single-instance lock (new):** on launch, the app checks a local lock file/OS mutex. If ROUTINGALL is already running, the second launch simply focuses the existing tray icon/window instead of starting a second listener.
- Cleanly terminates the listener on app exit.

### FR-3: Master Virtual Key Wrapper
- Purpose: blocks other local processes from silently using the user's real provider keys via the open local port. Not a defense against remote attackers (proxy never leaves loopback).
- Every request must present a valid Master Key via `Authorization: Bearer`, checked before any provider key is touched.

### FR-4: Model Routing Convention
- Requests specify `"<provider>/<model_name>"`.
- **Parsing rule (fixed):** split only on the **first** `/`. This correctly handles OpenRouter IDs that themselves contain a slash, e.g. `"openrouter/anthropic/claude-3-haiku"` → provider = `openrouter`, model = `anthropic/claude-3-haiku`.
- Missing or unrecognized prefix → `400` (see §8).

### FR-5: Key Validation on Entry (new)
When a user pastes a key into the GUI vault, it is validated against the known format for that provider **before** being written to the keychain:

| Provider | Expected Prefix Pattern |
|---|---|
| Groq | `gsk_...` |
| Google AI Studio | `AIzaSy...` |
| OpenRouter | `sk-or-v1-...` |

- Format check only — this does **not** verify the key is live/valid with the provider, only that it's not an obvious typo or wrong-provider paste.
- On mismatch, GUI shows an inline warning ("This doesn't look like a Groq key") but allows the user to save anyway, in case provider formats change — it's a guard rail, not a hard block.

### FR-7: In-App Code Snippet Viewer (new)
Settings includes a **Code** section showing ready-to-copy client setup snippets, so a developer never hand-types the base URL or Master Key.
- **Languages shown:** Python, Node.js/JavaScript, cURL (tabbed, one visible at a time).
- **Live substitution:** the snippet is generated with the user's *actual* current Master Key already filled in (masked by default, matching the Master Key section's reveal toggle) — not a placeholder the user has to find-and-replace themselves.
- **Copy button** per snippet, same interaction pattern as the endpoint row's existing copy button.
- **Snippet regenerates automatically** if the Master Key is rotated (FR "Regenerate"), so a stale key is never shown.
- Canonical snippet shape (Node.js example — Python and cURL follow the same base_url/apiKey pattern):
  ```javascript
  import OpenAI from "openai";

  const client = new OpenAI({
    baseURL: "http://127.0.0.1:8081/v1",
    apiKey: "rg-master-key-7f2a...",
  });

  const res = await client.chat.completions.create({
    model: "groq/llama-3.1-70b-versatile",
    messages: [{ role: "user", content: "Hello" }],
  });
  ```
- The `model` field in the shown snippet uses a real configured provider (whichever the user has set up first) rather than a placeholder string, so the example is copy-paste-runnable, not illustrative only.

---

## 6. Payload & Tool-Calling Translation (new — replaces vague "structural conversion")

### 6.1 Scope statement
v1 supports **text-based chat completion requests, including OpenAI-style tool/function calling**, translated per-provider as follows. Multimodal (`image_url`) handling is **selective, not a blanket block**:
- **Groq & OpenRouter:** both accept OpenAI's native `image_url` content-block format already, so multimodal requests are **passed through as-is** — no translation needed, no restriction imposed.
- **Gemini:** requires converting `image_url` into Gemini's own `inlineData` format, which is not yet built. Gemini multimodal requests are therefore **blocked with a `400`** until that conversion ships (§6.3, §8) — this is a deferred capability, not a permanent one.

This keeps the two providers that already work from being penalized by the one that doesn't.

### 6.2 Tool-calling translation table

| OpenAI Request Shape | Groq | Gemini | OpenRouter |
|---|---|---|---|
| `tools: [{type: "function", function: {...}}]` | Passed through as-is (Groq's API is OpenAI-compatible for tools) | **Translated**: each `function` entry is mapped into Gemini's `tools: [{functionDeclarations: [...]}]` structure; `parameters` JSON Schema is passed through largely as-is since Gemini accepts a JSON-Schema-like format | Passed through as-is (OpenRouter proxies OpenAI-shaped requests) |
| `tool_choice` | Passed through | Mapped to Gemini's `toolConfig.functionCallingConfig.mode` (`AUTO`/`ANY`/`NONE`) | Passed through |
| Response: `tool_calls` in assistant message | Native passthrough | Gemini's `functionCall` response parts are **translated back** into OpenAI's `tool_calls` array shape before streaming to the client | Native passthrough |

### 6.3 Parameters dropped or approximated per provider (documented, not silent)
- **Gemini:** `presence_penalty`, `frequency_penalty`, `logprobs`, and `seed` are not supported by Gemini's API. If present in the request, the proxy **strips them and adds a warning field** to the response metadata (`routingall_warnings: ["unsupported_param_dropped: presence_penalty"]`) rather than silently ignoring or erroring.
- **Multimodal content blocks (`image_url`), per provider:**
  - **Groq, OpenRouter:** forwarded as-is, unmodified — both providers accept OpenAI's `image_url` format natively, so no translation or blocking applies.
  - **Gemini:** rejected with `400` and a clear message (`"multimodal input requires inlineData conversion, not yet supported for Gemini"`) rather than forwarding a request Gemini's API will reject or mishandle in an OpenAI-shaped form. This is the one and only provider-specific multimodal restriction — it is not a general rule.

---

## 7. Health & Status Endpoint (new)

`GET http://127.0.0.1:8081/v1/health` (no Master Key required, since it reveals no secrets — only status):

```json
{
  "status": "ok",
  "version": "1.3.0",
  "port": 8081,
  "providers_configured": {
    "groq": true,
    "gemini": true,
    "openrouter": false
  },
  "keychain_access": "ok"
}
```

- `keychain_access` reports `"ok"`, `"denied"` (OS permission not granted), or `"error"`.
- `providers_configured` reflects whether a key is present for that provider — not whether it's currently valid with the provider (no live check, to avoid burning quota on a status call).
- GUI settings window surfaces this same data visually (green/red indicators per provider) so a developer can confirm setup without needing to hit the endpoint manually.

---

## 8. Error Handling (defined response body shape — new)

All errors return a body matching the OpenAI client SDK's expected error shape, so SDK-side error handling doesn't break:

```json
{
  "error": {
    "message": "No API key configured for provider: gemini",
    "type": "routingall_configuration_error",
    "code": "missing_provider_key"
  }
}
```

| Condition | HTTP Status | `error.type` |
|---|---|---|
| Missing/invalid Master Key | 401 | `authentication_error` |
| Model string missing/unrecognized provider prefix | 400 | `invalid_request_error` |
| Multimodal (`image_url`) content targeting **Gemini** specifically (not yet supported) | 400 | `invalid_request_error` |
| Recognized provider, no key configured | 424 | `routingall_configuration_error` |
| Provider call exceeds timeout (§9.4) | 504 | `routingall_timeout_error` |
| Provider itself returns an error | passthrough | passthrough (provider's own body, wrapped if shape mismatches) |

---

## 9. Security & Privacy

### 9.1 Zero Data Exfiltration
Request/response data flows only between local client code, the proxy, and the chosen provider.

### 9.2 Credential Isolation
Real keys live only in the OS keychain, never in `.env` files or repos.

### 9.3 Uninstall / Key Cleanup (new)
- **Problem:** most OS uninstallers (drag-to-trash on macOS, standard Windows uninstall) do **not** run application code, so keychain entries created by the app can be silently orphaned.
- **Fix, two layers:**
  1. **In-app "Reset & Remove All Keys" action** in the GUI settings screen, available at any time — deletes all provider keys from the OS keychain and invalidates the current Master Key immediately. This is the primary, reliable path.
  2. **Uninstall-time prompt (best effort):** on platforms where the packaging format supports an uninstall hook (Windows via NSIS/MSI custom action; macOS only if distributed outside the sandboxed App Store flow), the uninstaller runs the same cleanup routine automatically. Where the platform doesn't support this (e.g., simple `.app` drag-to-trash with no hook), the app's README and first-run screen both state explicitly: *"Uninstalling does not remove your saved API keys from the OS keychain — use Settings → Reset & Remove All Keys before uninstalling."*
- This makes cleanup guaranteed-available (in-app) and best-effort-automatic (uninstaller), rather than assuming a hook that may not exist.

### 9.4 Provider Call Timeouts (new)
- Every outbound provider request has a 60-second default timeout, configurable per-provider in GUI settings (useful since Gemini/Groq/OpenRouter have different typical latencies).
- On timeout, the proxy aborts the connection to the provider and returns `504` with `routingall_timeout_error` (§8) rather than holding the client connection open indefinitely.

### 9.5 Loopback-Only Guarantee
Refuses to start if it cannot bind exclusively to `127.0.0.1`.

---

## 10. Out of Scope (Explicitly Excluded)
- Gemini `inlineData` multimodal translation only — deferred, rejected cleanly at request time (§6.3, §8). Groq and OpenRouter multimodal (`image_url`) is in scope and passed through natively; audio input remains out of scope for all three providers.
- Automated model alias fallback or remote sync servers.
- Custom language SDK packages.
- Payload inspectors or debugging loggers.
- Local prompt caching.
- CORS/browser-client support (flagged as open, not solved — see Technical Specs table).

---

## 11. Changelog from v1.2
1. **Tool-calling translation (§6):** explicit per-provider mapping table for `tools`/`tool_choice`/`tool_calls`, plus a documented list of dropped/unsupported parameters returned as warnings, not silent drops.
2. **Health endpoint (§7):** `GET /v1/health` reporting proxy status, per-provider key presence, and keychain access state; mirrored in the GUI.
3. **Uninstall/key cleanup (§9.3):** guaranteed in-app "Reset & Remove All Keys" action plus best-effort uninstall hook where the platform supports it, with explicit user-facing documentation where it doesn't.
4. **Key format validation on entry (FR-5):** pattern-checks pasted keys against known provider prefixes, with a soft (non-blocking) warning on mismatch.
5. **Single-instance lock (FR-2):** prevents two running copies from fighting over port 8081.
6. **Port conflict handling (FR-2):** explicit GUI error + manual override instead of silent auto-fallback.
7. **Provider call timeouts (§9.4):** 60s default, configurable, returns a defined `504` error rather than hanging.
8. **Defined error body shape (§8):** all errors now match OpenAI SDK-expected JSON structure, with a `routingall_*` type namespace for proxy-originated errors.
9. **CORS:** explicitly left open/unresolved per current request, documented as a known gap rather than silently absent.

## 12. Changelog from v1.3 → v1.4
10. **Window close vs. Quit semantics (FR-1a):** resolves the ambiguity between FR-1 ("minimizes to tray") and FR-2 ("terminates on exit") by defining Close (X) as tray-minimize-only, and Quit as the sole action that actually kills the proxy listener, with a tray icon state indicator.

## 13. Changelog from v1.4 → v1.5 (dated 2026-08-11)
11. **Multimodal logic changed from blanket block to selective passthrough (§3, §6.1, §6.3, §8):** Groq and OpenRouter both accept OpenAI-format `image_url` requests natively and are no longer blocked — they pass through unmodified, same as any text request. Only **Gemini** blocks `image_url` requests (`400`), and only because its `inlineData` conversion hasn't been built yet — this is a provider-specific, temporary restriction, not a system-wide multimodal ban. See the companion design doc, `ROUTINGALL_Multimodal_Design_v1.md`, for the full rationale.

## 14. Changelog from v1.5 → v1.6 (dated 2026-08-11)
12. **In-app Code Snippet Viewer added (FR-7):** Settings now includes a tabbed Python/Node.js/cURL snippet section with the user's live Master Key pre-filled and a per-language copy button, so setup requires no manual find-and-replace. See `ROUTINGALL_Frontend_Schema.md` for the exact component spec.
