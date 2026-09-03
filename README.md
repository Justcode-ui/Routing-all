# ROUTINGALL

**Lightweight Local Developer Proxy and Master-Key Gateway**  
Version: 1.6.0  
License: MIT

ROUTINGALL is a local developer proxy that unifies multiple AI providers—Groq, Google AI Studio (Gemini), and OpenRouter—behind a single Master Virtual Key and an OpenAI-compatible HTTP interface running at `http://127.0.0.1:8081/v1`.

Point any OpenAI SDK, framework, or tool at the local endpoint and route requests to any underlying provider using a simple model prefix syntax (`provider/model`). Real API keys remain securely inside your OS keychain and are never hardcoded into project `.env` files or repositories.

---

## Table of Contents

- [Core Architecture](#core-architecture)
- [Quick Start](#quick-start)
- [Model Routing Syntax](#model-routing-syntax)
- [Developer Integration Examples](#developer-integration-examples)
  - [Python (OpenAI SDK)](#python-openai-sdk)
  - [Node.js / TypeScript](#nodejs--typescript)
  - [cURL](#curl)
- [API Reference](#api-reference)
  - [POST /v1/chat/completions](#post-v1chatcompletions)
  - [GET /v1/health](#get-v1health)
  - [GET /](#get-)
- [Request Activity & Rate Limit Visibility](#request-activity--rate-limit-visibility)
- [Moving to Production (`client.py`)](#moving-to-production-clientpy)
- [Security Model](#security-model)
- [Building from Source](#building-from-source)
- [Troubleshooting & Maintenance](#troubleshooting--maintenance)

---

## Core Architecture

ROUTINGALL operates as a desktop service built in Rust (Tauri + Axum + Reqwest):

1. **Loopback Isolation**: Binds strictly to `127.0.0.1:8081`. It is inaccessible to any external network or device.
2. **Master Virtual Key**: Your local code authenticates against ROUTINGALL using a generated master key (`rg-master-key-...`). If compromised, rotate it instantly in Settings without changing upstream provider keys.
3. **OS Keychain Adapter**: Upstream provider API keys (Groq, Gemini, OpenRouter) are encrypted in the OS Credential Manager (Windows), Keychain (macOS), or Secret Service (Linux). Keys are never written to disk files or databases.
4. **Translation Engine**: Translates OpenAI chat completion schemas into native provider protocols on the fly, including non-streaming completions, server-sent event (SSE) streams, and function/tool calling.

---

## Quick Start

### 1. Launch the Application
Run the ROUTINGALL application. The system tray icon will appear and the local proxy will bind to port `8081`.

### 2. Configure Provider Keys
Open the Settings window from the system tray or menu:
- Add your **Groq** API key (`gsk_...`)
- Add your **Google AI Studio** key (`AIzaSy...`)
- Add your **OpenRouter** key (`sk-or-v1-...`)

### 3. Copy Your Master Key
Click the Master Key field in the Settings window to copy your virtual key (`rg-master-key-...`).

### 4. Direct Your Code to Localhost
Configure your OpenAI client to target `http://127.0.0.1:8081/v1`:

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8081/v1",
    api_key="rg-master-key-your-local-key"
)

response = client.chat.completions.create(
    model="groq/llama-3.3-70b-versatile",
    messages=[{"role": "user", "content": "Hello world"}]
)

print(response.choices[0].message.content)
```

---

## Model Routing Syntax

ROUTINGALL inspects the `model` field in your payload and splits it on the first forward slash (`/`):

`"<provider>/<target_model>"`

| Provider | Prefix Syntax | Target Model Example | Full Model String |
|---|---|---|---|
| **Groq** | `groq/` | `llama-3.3-70b-versatile` | `groq/llama-3.3-70b-versatile` |
| **Groq** | `groq/` | `llama-3.1-8b-instant` | `groq/llama-3.1-8b-instant` |
| **Google AI Studio** | `gemini/` or `google/` | `gemini-2.0-flash` | `gemini/gemini-2.0-flash` |
| **Google AI Studio** | `gemini/` or `google/` | `gemini-1.5-pro` | `gemini/gemini-1.5-pro` |
| **OpenRouter** | `openrouter/` | `anthropic/claude-3.5-sonnet` | `openrouter/anthropic/claude-3.5-sonnet` |
| **OpenRouter** | `openrouter/` | `meta-llama/llama-3.3-70b-instruct` | `openrouter/meta-llama/llama-3.3-70b-instruct` |

Note: Parameters such as `max_tokens` or `temperature` must be supplied as top-level API parameters, not concatenated into the model string.

---

## Developer Integration Examples

### Python (OpenAI SDK)

```python
import os
from openai import OpenAI

# Initialize client pointing to ROUTINGALL
client = OpenAI(
    base_url="http://127.0.0.1:8081/v1",
    api_key=os.environ.get("ROUTINGALL_KEY", "rg-master-key-your-key-here")
)

# 1. Non-streaming Chat Completion
def ask_model(prompt: str, model: str = "groq/llama-3.3-70b-versatile") -> str:
    response = client.chat.completions.create(
        model=model,
        messages=[{"role": "user", "content": prompt}],
        temperature=0.7,
        max_tokens=1000
    )
    return response.choices[0].message.content

# 2. Streaming Chat Completion
def stream_model(prompt: str, model: str = "gemini/gemini-2.0-flash"):
    stream = client.chat.completions.create(
        model=model,
        messages=[{"role": "user", "content": prompt}],
        stream=True
    )
    for chunk in stream:
        if chunk.choices and chunk.choices[0].delta.content:
            print(chunk.choices[0].delta.content, end="", flush=True)
    print()

# 3. Tool / Function Calling (Gemini or Groq)
def run_tool_call():
    tools = [
        {
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get the current weather for a city",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "location": {"type": "string", "description": "City name"}
                    },
                    "required": ["location"]
                }
            }
        }
    ]
    response = client.chat.completions.create(
        model="gemini/gemini-2.0-flash",
        messages=[{"role": "user", "content": "What is the weather in Tokyo?"}],
        tools=tools
    )
    print(response.choices[0].message.tool_calls)
```

---

### Node.js / TypeScript

```typescript
import OpenAI from "openai";

const client = new OpenAI({
  baseURL: "http://127.0.0.1:8081/v1",
  apiKey: process.env.ROUTINGALL_KEY || "rg-master-key-your-key-here",
});

async function main() {
  // Standard call
  const completion = await client.chat.completions.create({
    model: "groq/llama-3.3-70b-versatile",
    messages: [{ role: "user", content: "Explain vector databases in two sentences." }],
  });
  console.log(completion.choices[0].message.content);

  // Streaming call
  const stream = await client.chat.completions.create({
    model: "openrouter/anthropic/claude-3.5-sonnet",
    messages: [{ role: "user", content: "Count from 1 to 5." }],
    stream: true,
  });

  for await (const chunk of stream) {
    process.stdout.write(chunk.choices[0]?.delta?.content || "");
  }
}

main().catch(console.error);
```

---

### cURL

```bash
# Chat Completion request
curl -X POST http://127.0.0.1:8081/v1/chat/completions \
  -H "Authorization: Bearer rg-master-key-your-key-here" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gemini/gemini-2.0-flash",
    "messages": [
      {"role": "system", "content": "You are a concise engineering assistant."},
      {"role": "user", "content": "What is the capital of France?"}
    ],
    "temperature": 0.2
  }'
```

---

## API Reference

### POST /v1/chat/completions
The primary proxy endpoint conforming to the OpenAI Chat Completions specification.

- **Headers**:
  - `Authorization: Bearer <master_key>` (Required)
  - `Content-Type: application/json`
- **Request Body**: Standard OpenAI parameters (`model`, `messages`, `temperature`, `max_tokens`, `stream`, `tools`, `stop`, etc.).
- **Response**: Standard OpenAI completion object or `text/event-stream` stream.

---

### GET /v1/health
Returns server status, configuration state, and session activity tracking. Does not require authentication.

```bash
curl http://127.0.0.1:8081/v1/health
```

**Response Example:**
```json
{
  "status": "ok",
  "version": "1.6.0",
  "port": 8081,
  "is_listening": true,
  "error": null,
  "providers_configured": {
    "groq": true,
    "gemini": true,
    "openrouter": false
  },
  "keychain_access": "ok",
  "usage": {
    "groq": {
      "requests_this_session": 24,
      "requests_last_hour": 10,
      "rate_limit_remaining": 58
    },
    "gemini": {
      "requests_this_session": 12,
      "requests_last_hour": 4,
      "rate_limit_remaining": null
    },
    "openrouter": {
      "requests_this_session": 0,
      "requests_last_hour": 0,
      "rate_limit_remaining": null
    }
  }
}
```

---

### GET /
Returns service discovery metadata and endpoint URLs.

```json
{
  "service": "ROUTINGALL",
  "version": "1.6.0",
  "status": "listening",
  "port": 8081,
  "description": "Lightweight Local Developer Proxy & Master-Key Gateway",
  "endpoints": {
    "health": "http://127.0.0.1:8081/v1/health",
    "chat_completions": "http://127.0.0.1:8081/v1/chat/completions"
  }
}
```

---

## Request Activity & Rate Limit Visibility

ROUTINGALL monitors request volumes and upstream provider headers in memory:

1. **Session Request Counts**: Tracks total routed requests per provider for the active session. State resides in memory and resets on application restart.
2. **Best-Effort Rate-Limit Extraction**: When an upstream provider returns standard rate limit headers (such as `x-ratelimit-remaining-requests` from Groq), ROUTINGALL captures the most recent value and displays it in the Settings UI.
3. **Observational Guard**: ROUTINGALL does not throttle, queue, or alter routing decisions based on rate limits.

---

## Moving to Production (`client.py`)

When deploying your backend to a production environment (AWS, Vercel, Railway, Fly.io, etc.), you do not run the ROUTINGALL desktop app. Instead, use the included `client.py` module as a direct drop-in replacement.

`client.py` preserves the identical `provider/model` routing syntax while communicating directly with upstream provider APIs using host environment variables.

### 1. Set Environment Variables on Host
```bash
export GROQ_API_KEY="gsk_..."
export GEMINI_API_KEY="AIzaSy..."
export OPENROUTER_API_KEY="sk-or-v1-..."
```

### 2. Install Dependencies
```bash
pip install openai google-generativeai
```

### 3. Usage in Application
```python
from client import call

# Exact same routing call structure
response = call(
    "gemini/gemini-2.0-flash",
    messages=[{"role": "user", "content": "Summarize this data."}]
)
```

---

## Security Model

- **Localhost Loopback Only**: The server explicitly binds to `127.0.0.1` and ignores external interface bindings.
- **Keychain Storage**: API keys are passed directly to the operating system's native credential store. No database, plain text file, or cache file is created.
- **Zero Log Persistence**: Request payloads and completions are streamed through memory and never written to disk logs.
- **Immediate Invalidation**: The "Reset & Remove All Keys" button in Settings immediately purges all keys from the OS keychain and rotates the Master Key.

---

## Building from Source

### Prerequisites
- **Node.js**: v18.0.0 or higher
- **Rust**: Latest stable toolchain (`rustup default stable`)
- **C/C++ Build Tools**:
  - Windows: MSVC C++ Build Tools or WinLibs MinGW
  - macOS: Xcode Command Line Tools
  - Linux: `build-essential`, `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`

### Development Mode
```bash
# Clone the repository
git clone https://github.com/your-org/routingall.git
cd routingall

# Install frontend dependencies
npm install

# Start Tauri development environment
npm run dev
```

### Production Build
```bash
# Compile optimized binaries and platform installer
npm run build
```
Built binaries and installers are output to `target/release/` and `src-tauri/target/release/bundle/`.

---

## Troubleshooting & Maintenance

### Port Conflict (Port 8081 in use)
If port 8081 is occupied by another local service:
1. The status indicator in Settings will turn red and display `PORT CONFLICT`.
2. Terminate the process using port 8081:
   - Windows PowerShell: `Get-Process -Id (Get-NetTCPConnection -LocalPort 8081).OwningProcess | Stop-Process`
   - macOS / Linux: `kill -9 $(lsof -t -i:8081)`
3. Restart ROUTINGALL.

### Unrecognized Provider Prefix Error
Ensure model names start with a supported prefix followed by a slash:
- Valid: `groq/llama-3.3-70b-versatile`, `gemini/gemini-2.0-flash`, `openrouter/anthropic/claude-3.5-sonnet`
- Invalid: `llama-3.3-70b`, `googleai/gemini-2.0-flash`

### Uninstallation & Cleanup
Before removing the application binary, open **Settings -> Reset & Remove All Keys** to delete stored keys from the OS credential store. Standard operating system uninstallation does not automatically modify OS credential vaults.
