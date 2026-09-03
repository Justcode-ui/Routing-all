"""
gemini_text_client.py — production text-only client for Gemini.
Companion module to ROUTINGALL Production Router.

Uses Google's OpenAI-compatibility endpoint for text chat completions when no tools
or function calls are required. Refuses to accept tools or multimodal input.
"""

import os
from openai import OpenAI


def get_gemini_text_client() -> OpenAI:
    """Returns an OpenAI client configured for Google AI Studio's OpenAI-compatibility endpoint."""
    return OpenAI(
        base_url="https://generativelanguage.googleapis.com/v1beta/openai/",
        api_key=os.environ["GEMINI_API_KEY"],
    )


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


def call(model: str, messages, tools=None, **kwargs):
    """
    Executes a text-only chat completion call to Gemini.
    Raises ValueError immediately if tools are provided or if image_url blocks are present.
    """
    if tools:
        raise ValueError(
            "Gemini Text Client does not support tools — use the Gemini Tool-Calling Client instead "
            "(gemini_text_client_no_tools)"
        )
    _reject_images(messages)
    return get_gemini_text_client().chat.completions.create(model=model, messages=messages, **kwargs)
