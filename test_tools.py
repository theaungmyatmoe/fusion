import os
import requests
import json

account_id = os.environ.get("CLOUDFLARE_ACCOUNT_ID")
api_token = os.environ.get("CLOUDFLARE_API_TOKEN")

print(f"Account ID: {account_id}")

tools = [
    {
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read the contents of a file.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }
        }
    },
    {
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Write a new file.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }
        }
    },
    {
        "type": "function",
        "function": {
            "name": "search_replace",
            "description": "Search and replace text.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old_string": { "type": "string" },
                    "new_string": { "type": "string" }
                },
                "required": ["path", "old_string", "new_string"]
            }
        }
    },
    {
        "type": "function",
        "function": {
            "name": "grep",
            "description": "Search code using ripgrep.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" }
                },
                "required": ["pattern"]
            }
        }
    },
    {
        "type": "function",
        "function": {
            "name": "run_command",
            "description": "Run shell command.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            }
        }
    }
]

url = f"https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1/chat/completions"
headers = {
    "Authorization": f"Bearer {api_token}",
    "Content-Type": "application/json"
}

# Try Qwen with tools
payload = {
    "model": "@cf/moonshotai/kimi-k2.7-code",
    "messages": [
        {"role": "user", "content": "create a reactjs todo list project"}
    ],
    "tools": tools,
    "max_tokens": 16384
}

print("Sending request to Qwen 2.5 Coder with tools...")
try:
    response = requests.post(url, headers=headers, json=payload, timeout=45)
    print(f"Status Code: {response.status_code}")
    print(f"Response: {response.text}")
except Exception as e:
    print(f"Error: {e}")
