"""
gemini_tools_client.py — production tool-calling client for Gemini.
Companion module to ROUTINGALL Production Router.

Uses Gemini's native SDK (google-generativeai) for reliable function calling without
relying on an OpenAI-compatibility layer. Translates schemas and response shapes.
"""

import os


def get_gemini_native_client(model: str):
    """Initializes the native Google GenerativeAI model instance."""
    import google.generativeai as genai
    genai.configure(api_key=os.environ["GEMINI_API_KEY"])
    return genai.GenerativeModel(model)


def _reject_images(messages):
    """Rejects requests containing image_url content blocks for Gemini."""
    for msg in messages:
        content = msg.get("content")
        if isinstance(content, list):
            for block in content:
                if isinstance(block, dict) and block.get("type") == "image_url":
                    raise ValueError(
                        "multimodal input requires inlineData conversion, not yet supported for Gemini "
                        "(gemini_multimodal_unsupported)"
                    )


def _translate_tools_to_gemini(openai_tools):
    """Translates OpenAI tool definitions to Gemini function_declarations shape."""
    declarations = []
    for t in openai_tools:
        if t.get("type") == "function":
            fn = t["function"]
            declarations.append({
                "name": fn["name"],
                "description": fn.get("description", ""),
                "parameters": fn.get("parameters", {}),
            })
    return [{"function_declarations": declarations}] if declarations else None


def _translate_response_to_openai_shape(gemini_response):
    """Reshapes native Gemini response parts into OpenAI-compatible tool_calls shape."""
    candidate = gemini_response.candidates[0]
    tool_calls = []
    text_parts = []
    for part in candidate.content.parts:
        if hasattr(part, "function_call") and part.function_call:
            tool_calls.append({
                "type": "function",
                "function": {
                    "name": part.function_call.name,
                    "arguments": dict(part.function_call.args),
                },
            })
        elif hasattr(part, "text"):
            text_parts.append(part.text)
    return {
        "content": "".join(text_parts) or None,
        "tool_calls": tool_calls or None,
    }


def _messages_to_gemini_prompt(messages):
    """Flattens OpenAI message list to a combined prompt string for Gemini native SDK."""
    prompt_lines = []
    for m in messages:
        role = m.get("role", "user")
        content = m.get("content", "")
        if isinstance(content, str):
            prompt_lines.append(f"{role.capitalize()}: {content}")
    return "\n".join(prompt_lines)


def call(model: str, messages, tools=None, **kwargs):
    """
    Executes a tool-calling request via Gemini native SDK and returns an OpenAI-shaped dict response.
    """
    _reject_images(messages)
    client = get_gemini_native_client(model)
    gemini_tools = _translate_tools_to_gemini(tools) if tools else None
    prompt = _messages_to_gemini_prompt(messages)
    response = client.generate_content(prompt, tools=gemini_tools)
    return _translate_response_to_openai_shape(response)
