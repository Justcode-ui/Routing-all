"""
client.py — production AI router.
Companion to ROUTINGALL (local dev tool). This file replaces ROUTINGALL's
proxy once you deploy, using the same call shape: call("provider/model", messages).

Before pushing:
  1. Set GROQ_API_KEY, GEMINI_API_KEY, OPENROUTER_API_KEY as environment
     variables / secrets on your host (Vercel, AWS, Railway, etc.) — never
     commit real keys to your repo.
  2. pip install openai google-generativeai
"""

import os
from openai import OpenAI


# ── Provider clients (OpenAI-compatible endpoints) ──────────────────────────

def get_client(provider: str) -> OpenAI:
    if provider == "groq":
        return OpenAI(base_url="https://api.groq.com/openai/v1", api_key=os.environ["GROQ_API_KEY"])
    if provider == "gemini":
        return OpenAI(base_url="https://generativelanguage.googleapis.com/v1beta/openai/", api_key=os.environ["GEMINI_API_KEY"])
    if provider == "openrouter":
        return OpenAI(base_url="https://openrouter.ai/api/v1", api_key=os.environ["OPENROUTER_API_KEY"])
    raise ValueError(f"unknown provider: {provider}")


# ── Multimodal guard (Gemini only — mirrors ROUTINGALL PRD §6.1/§6.3) ──────

def _contains_image(messages) -> bool:
    for msg in messages:
        content = msg.get("content")
        if isinstance(content, list):
            for block in content:
                if isinstance(block, dict) and block.get("type") == "image_url":
                    return True
    return False


def _check_gemini_restrictions(provider, messages):
    if provider == "gemini" and _contains_image(messages):
        raise ValueError(
            "multimodal input requires inlineData conversion, not yet supported for Gemini "
            "(gemini_multimodal_unsupported)"
        )


# ── Main entry point ─────────────────────────────────────────────────────────

def call(model_string: str, messages, tools=None, **kwargs):
    """
    Single entry point for every provider. Same call shape as ROUTINGALL:
        call("groq/llama-3.1-70b-versatile", messages=[...])
        call("gemini/gemini-1.5-pro", messages=[...])
        call("gemini/gemini-1.5-pro", messages=[...], tools=[...])   # auto-routes to native SDK
        call("openrouter/anthropic/claude-3-haiku", messages=[...])
    """
    provider, model = model_string.split("/", 1)  # first-slash-only, matches ROUTINGALL FR-4
    _check_gemini_restrictions(provider, messages)

    if provider == "gemini" and tools:
        # Tool calls go through the dedicated native-SDK module — see
        # ROUTINGALL_Gemini_ToolCalling_PRD.md. Import kept local to avoid
        # requiring google-generativeai unless it's actually needed.
        from gemini_tools_client import call as gemini_tools_call
        return gemini_tools_call(model, messages, tools=tools, **kwargs)

    return get_client(provider).chat.completions.create(model=model, messages=messages, **kwargs)
