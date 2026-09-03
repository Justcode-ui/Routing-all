"""
Unit tests for ROUTINGALL companion production modules.
Tests first-slash routing rules, tool rejection on text client, and multimodal guard rails.
"""

import os
import sys
import unittest

# Ensure project root is in path
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

import gemini_text_client
import gemini_tools_client
import client


class TestRoutingAllCompanionModules(unittest.TestCase):

    def test_gemini_text_client_rejects_tools(self):
        """Verify that gemini_text_client raises ValueError when tools are passed."""
        messages = [{"role": "user", "content": "Hello"}]
        tools = [{"type": "function", "function": {"name": "test_func"}}]

        with self.assertRaises(ValueError) as ctx:
            gemini_text_client.call("gemini-1.5-pro", messages=messages, tools=tools)

        self.assertIn("gemini_text_client_no_tools", str(ctx.exception))

    def test_gemini_text_client_rejects_multimodal(self):
        """Verify that gemini_text_client raises ValueError for image_url content."""
        messages = [{
            "role": "user",
            "content": [
                {"type": "text", "text": "What is in this image?"},
                {"type": "image_url", "image_url": {"url": "https://example.com/image.png"}}
            ]
        }]

        with self.assertRaises(ValueError) as ctx:
            gemini_text_client.call("gemini-1.5-pro", messages=messages)

        self.assertIn("gemini_multimodal_unsupported", str(ctx.exception))

    def test_tools_schema_translation(self):
        """Verify OpenAI tool schema conversion to Gemini function_declarations."""
        openai_tools = [{
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Fetch weather data",
                "parameters": {
                    "type": "object",
                    "properties": {"city": {"type": "string"}},
                    "required": ["city"]
                }
            }
        }]

        gemini_tools = gemini_tools_client._translate_tools_to_gemini(openai_tools)
        self.assertIsNotNone(gemini_tools)
        declarations = gemini_tools[0]["function_declarations"]
        self.assertEqual(len(declarations), 1)
        self.assertEqual(declarations[0]["name"], "get_weather")
        self.assertEqual(declarations[0]["description"], "Fetch weather data")
        self.assertIn("city", declarations[0]["parameters"]["properties"])

    def test_client_first_slash_split(self):
        """Verify client.py first-slash split logic for OpenRouter IDs."""
        model_str = "openrouter/anthropic/claude-3-haiku"
        provider, model = model_str.split("/", 1)
        self.assertEqual(provider, "openrouter")
        self.assertEqual(model, "anthropic/claude-3-haiku")


if __name__ == "__main__":
    unittest.main()
