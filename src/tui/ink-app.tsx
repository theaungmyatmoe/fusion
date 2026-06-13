import React, { useState } from 'react';
import { render, Box, Text, useInput, useApp } from 'ink';
import TextInput from 'ink-text-input';
import { createLLMClient, type ChatMessage } from '../llm/client.js';
import type { Config } from '../config.js';

interface Props {
  config: Config;
  initialMessage?: string;
}

interface Message {
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
}

export const InkApp: React.FC<Props> = ({ config, initialMessage }) => {
  const { exit } = useApp();
  const llm = React.useMemo(() => createLLMClient(config), [config]);

  const [messages, setMessages] = useState<Message[]>([
    {
      role: 'system',
      content: `zencode (Ink TUI) — Termux-friendly rich mode\nDefault: ${config.model} via Cloudflare\nType /help • Ctrl+C to quit`,
    },
  ]);
  const [input, setInput] = useState('');
  const [isThinking, setIsThinking] = useState(false);

  const addMessage = (msg: Message) => {
    setMessages((prev) => [...prev, msg]);
  };

  const handleSubmit = async (text: string) => {
    if (!text.trim()) return;

    addMessage({ role: 'user', content: text.trim() });
    setInput('');
    setIsThinking(true);

    const lower = text.toLowerCase().trim();

    // Slash commands (work without model)
    if (lower.startsWith('/')) {
      if (lower === '/help') {
        addMessage({
          role: 'system',
          content: 'Commands: /help /yolo /plan /model <name> /status /exit\nThis is the Ink rich TUI (desktop-friendly). In Termux the simple REPL is often better.',
        });
      } else if (lower === '/yolo') {
        addMessage({ role: 'system', content: 'YOLO mode toggled (stub in this UI)' });
      } else if (lower === '/plan') {
        addMessage({ role: 'system', content: 'Plan mode (stub): agent will explore without editing.' });
      } else if (lower.startsWith('/model ')) {
        const newModel = lower.split(' ')[1];
        addMessage({ role: 'system', content: `Model would switch to ${newModel} (restart with ZENCODE_MODEL=...)` });
      } else if (lower === '/status') {
        addMessage({ role: 'system', content: `model=${config.model}  (Cloudflare Kimi/GLM default)` });
      } else if (lower === '/exit') {
        exit();
        return;
      } else {
        addMessage({ role: 'system', content: `Unknown command: ${lower}. Try /help` });
      }
      setIsThinking(false);
      return;
    }

    // Special demo cases (no LLM needed)
    if (lower.includes('edit') || lower.includes('fix') || lower.includes('change')) {
      addMessage({
        role: 'tool',
        content: 'search_replace demo (unique old_string safety):\n```diff\n+ // mobile Termux change\n- // old\n```',
      });
      addMessage({ role: 'assistant', content: 'Precise edit ready. In full agent this applies after approval.' });
      setIsThinking(false);
      return;
    }

    if (lower.includes('todo') || lower.includes('plan')) {
      addMessage({
        role: 'tool',
        content: '[todo_write]\n- [ ] Mobile Termux support\n- [x] Ink rich TUI\n- [ ] Full tool calling + sub-agents',
      });
      addMessage({ role: 'assistant', content: 'Todo list updated.' });
      setIsThinking(false);
      return;
    }

    // Real LLM call (Kimi 2.7 / GLM / Grok depending on config)
    try {
      const llmMessages: ChatMessage[] = [
        {
          role: 'system',
          content: 'You are Zencode, a practical coding agent. Be concise. Prefer small, safe edits. Optimized for Termux mobile.',
        },
        { role: 'user', content: text },
      ];

      const result = await llm.chat({ messages: llmMessages, temperature: 0.6 });
      addMessage({ role: 'assistant', content: result.content || '(no content)' });
    } catch (err: any) {
      addMessage({
        role: 'assistant',
        content: `LLM error: ${err?.message || err}\n\nTip: export CLOUDFLARE_ACCOUNT_ID + CLOUDFLARE_API_TOKEN for default Kimi 2.7 route.`,
      });
    }

    setIsThinking(false);
  };

  // Handle initial message
  React.useEffect(() => {
    if (initialMessage) {
      handleSubmit(initialMessage);
    }
  }, []);

  // Keyboard shortcuts
  useInput((inputKey, key) => {
    if (key.ctrl && inputKey === 'c') {
      exit();
    }
    if (key.return && input.trim()) {
      handleSubmit(input);
    }
  });

  return (
    <Box flexDirection="column" height="100%">
      {/* Header */}
      <Box borderStyle="round" paddingX={1}>
        <Text color="magentaBright" bold>zencode</Text>
        <Text color="gray">  (Ink mode • {config.model})</Text>
        <Text color="gray">  • /help • Ctrl+C exit</Text>
      </Box>

      {/* Messages area */}
      <Box flexDirection="column" flexGrow={1} paddingX={1} overflow="hidden">
        {messages.map((msg, i) => {
          const color =
            msg.role === 'user' ? 'blue' :
            msg.role === 'assistant' ? 'green' :
            msg.role === 'tool' ? 'magenta' : 'gray';
          const prefix = msg.role === 'user' ? 'You' : msg.role === 'assistant' ? 'Agent' : msg.role === 'tool' ? 'Tool' : 'System';
          return (
            <Box key={i} marginBottom={1}>
              <Text color={color} bold>{prefix}: </Text>
              <Text color={color} wrap="truncate-end">{msg.content}</Text>
            </Box>
          );
        })}
        {isThinking && <Text color="gray">Agent thinking...</Text>}
      </Box>

      {/* Input */}
      <Box borderStyle="single" paddingX={1}>
        <Text color="cyan">z&gt; </Text>
        <TextInput
          value={input}
          onChange={setInput}
          onSubmit={() => handleSubmit(input)}
          placeholder="Type message or /command..."
        />
      </Box>

      <Text color="gray" dimColor>Tip: On Termux the simple REPL (default) is usually more practical than rich TUIs.</Text>
    </Box>
  );
};

export function runInkTUI(config: Config, initialMessage?: string) {
  render(<InkApp config={config} initialMessage={initialMessage} />);
}
