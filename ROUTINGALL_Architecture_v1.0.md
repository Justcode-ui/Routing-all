# ROUTINGALL — Design & Architecture Document (v1.0)

Companion to `ROUTINGALL_PRD_v1.4.md`. This document covers *how* the system is built, not *what* it does — process model, data flow, module boundaries, and implementation decisions.

---

## 1. Process Model

```
┌───────────────────────────────────────────────────────────────────┐
│                         OS (macOS / Windows / Linux)                │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                     ROUTINGALL.app (Tauri)                    │   │
│  │                                                                 │   │
│  │  ┌───────────────┐        ┌───────────────────────────────┐   │   │
│  │  │  Frontend       │        │  Backend (Rust)                 │   │   │
│  │  │  (WebView)       │◀──IPC─▶│                                  │   │   │
│  │  │                   │        │  ┌───────────────────────────┐  │   │   │
│  │  │  - Settings GUI   │        │  │  Tray Controller             │  │   │   │
│  │  │  - Key vault form │        │  │  (owns window show/hide,     │  │   │   │
│  │  │  - Status panel   │        │  │   quit lifecycle — FR-1a)     │  │   │   │
│  │  └───────────────────┘        │  └───────────────────────────┘  │   │   │
│  │                                 │  ┌───────────────────────────┐  │   │   │
│  │                                 │  │  HTTP Proxy Server           │  │   │   │
│  │                                 │  │  (binds 127.0.0.1:8081)     │  │   │   │
│  │                                 │  └───────────────────────────┘  │   │   │
│  │                                 │  ┌───────────────────────────┐  │   │   │
│  │                                 │  │  Keychain Adapter             │  │   │   │
│  │                                 │  │  (OS-native credential API)   │  │   │   │
│  │                                 │  └───────────────────────────┘  │   │   │
│  │                                 │  ┌───────────────────────────┐  │   │   │
│  │                                 │  │  Single-Instance Lock         │  │   │   │
│  │                                 │  └───────────────────────────┘  │   │   │
│  │                                 └───────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                       │                               │
│                                  loopback only                        │
│                                       ▼                               │
│                        127.0.0.1:8081 (dev scripts hit this)          │
└───────────────────────────────────────────────────────────────────┘
```

**Key decision:** the HTTP proxy server and the GUI are two logical components inside **one process** (standard Tauri model — Rust backend + WebView frontend, no separate child process for the proxy). This keeps the single-instance lock trivial (§4) and avoids IPC overhead between a "GUI process" and a "proxy process."

---

## 2. Request Lifecycle (Sequence)

```
Dev Script                Proxy (Rust)              Keychain           Provider (Groq/Gemini/OpenRouter)
    │                          │                        │                          │
    │  POST /v1/chat/          │                        │                          │
    │  completions             │                        │                          │
    │  Authorization: Bearer   │                        │                          │
    │  rg-master-key-...       │                        │                          │
    │─────────────────────────▶│                        │                          │
    │                          │  1. Validate Master Key │                          │
    │                          │     (in-memory compare, │                          │
    │                          │      not keychain)      │                          │
    │                          │                        │                          │
    │                          │  2. Parse model prefix │                          │
    │                          │     "groq/llama-3.1"   │                          │
    │                          │     → first-slash split │                          │
    │                          │                        │                          │
    │                          │  3. Lookup provider key │                          │
    │                          │────────────────────────▶│                          │
    │                          │◀────────────────────────│                          │
    │                          │                        │                          │
    │                          │  4. Translate payload   │                          │
    │                          │     (tools, params —    │                          │
    │                          │      PRD §6)            │                          │
    │                          │                        │                          │
    │                          │  5. Forward request     │                          │
    │                          │     (with timeout,      │                          │
    │                          │      PRD §9.4)          │                          │
    │                          │───────────────────────────────────────────────────▶│
    │                          │                        │                          │
    │                          │  6. Stream response,    │                          │
    │                          │     translate back if   │                          │
    │                          │     needed (Gemini)     │                          │
    │                          │◀───────────────────────────────────────────────────│
    │◀─────────────────────────│                        │                          │
    │  SSE stream (OpenAI-      │                        │                          │
    │  compatible shape)        │                        │                          │
```

**Note:** the Master Key check (step 1) is an in-memory string comparison against the key generated at first launch — it is never itself stored in the OS keychain, since it's not a secret shared with a third party, just a local access gate. It's held in the Rust process's memory and persisted to local (non-keychain) app config so it survives restarts.

---

## 3. Module Breakdown

| Module | Responsibility | Notes |
|---|---|---|
| `tray_controller` | Owns window show/hide/quit lifecycle (FR-1a), renders running/stopped state on tray icon | Only place that can actually terminate the listener |
| `proxy_server` | Binds `127.0.0.1:8081`, routes `/v1/chat/completions` and `/v1/health` | Single Axum/Actix (or equivalent) HTTP server instance |
| `auth` | Master Key generation, validation, rotation | In-memory + local config, not keychain (see §2 note) |
| `router` | Parses `provider/model` prefix (first-slash split), maps to provider adapter | Pure function, easily unit-testable |
| `keychain_adapter` | Wraps OS-native credential API (Keychain/Credential Manager/Secret Service) behind one trait | Enables FR-5 validation and §9.3 cleanup to call one interface regardless of OS |
| `translators/groq`, `translators/gemini`, `translators/openrouter` | Per-provider request/response shape translation (PRD §6) | Gemini is the only one requiring real transformation; Groq/OpenRouter are near-passthrough |
| `single_instance` | OS mutex / lock file check on launch | Prevents two proxies fighting over the port (PRD FR-2) |
| `error_shape` | Normalizes all error responses to the OpenAI SDK-expected JSON body (PRD §8) | One shared formatter used by every failure path |

---

## 4. Single-Instance Lock — Implementation Approach

- **macOS/Linux:** advisory file lock (`flock`) on a file in the app's local data directory. Second launch attempts the same lock, fails immediately, and instead sends an IPC signal (local socket or the OS's native "second-instance" API in Tauri) to the already-running instance to focus its window.
- **Windows:** named mutex (`CreateMutexW`), same fallback behavior on second-launch detection.
- Tauri has built-in single-instance plugin support that wraps both of these — recommended over hand-rolling it.

---

## 5. Data at Rest

| Data | Storage | Notes |
|---|---|---|
| Provider API keys (Groq/Gemini/OpenRouter) | OS keychain only | Per PRD §5.3/9.2 — never written to disk in plaintext or app config |
| Master Key | Local app config file (not keychain) | Not a third-party secret; regenerable any time from GUI |
| Port override (if 8081 is taken) | Local app config file | Plaintext is fine, no sensitive data |
| Provider timeout settings | Local app config file | Plaintext is fine |

No database, no telemetry, no analytics calls — consistent with the PRD's zero-cloud non-goal.

---

## 6. Streaming Translation Detail (Gemini)

This is the highest-risk implementation piece flagged in the PRD, spelled out here:

1. Client sends an OpenAI-format request with `"stream": true`.
2. Proxy translates the request body into Gemini's REST shape (including tool declarations per PRD §6.2) and calls Gemini's `streamGenerateContent` endpoint.
3. Gemini returns newline-delimited JSON chunks (not SSE `data:` framing).
4. Proxy's Gemini translator re-frames each chunk into an OpenAI-style SSE event (`data: {...}\n\n`), mapping Gemini's `candidates[].content.parts[].functionCall` into OpenAI's `choices[].delta.tool_calls` incrementally, and emits a final `data: [DONE]\n\n` the client SDK expects.
5. Groq and OpenRouter skip step 4 entirely — their native streaming format is already OpenAI-shaped SSE, so the proxy passes bytes through with only the Master Key/routing overhead applied upstream.

---

## 7. Backlog / Deferred (not blocking v1 build)

- **Auto-launch at system login** — convenience feature, not required for core function; flagged during this session as worth deciding later.
- **CORS support** — deferred per PRD, only relevant if browser-based clients become a target.
- **Live key validation** (calling the provider to confirm a key actually works, vs. just format-checking) — could be a manual "Test Connection" button in a future version.

---

## 8. Build Order Recommendation

1. `proxy_server` + `router` + `auth` — get a working loopback proxy with Master Key gating and prefix routing (no translation yet, Groq/OpenRouter passthrough only).
2. `keychain_adapter` + GUI key vault + FR-5 validation.
3. `single_instance` + `tray_controller` + FR-1a close/quit semantics.
4. `/v1/health` endpoint + GUI status panel.
5. `translators/gemini` (the hard part — request shape + streaming re-framing, §6).
6. `error_shape` normalization across all failure paths.
7. Timeout handling, port-conflict GUI flow, uninstall cleanup action.

This order front-loads the riskiest piece (Gemini translation) after the scaffolding is solid enough to test against it easily, rather than first or last.
