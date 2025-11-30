import { spawn } from 'child_process';
import express from 'express';
import stripAnsi from 'strip-ansi';

interface ChatMessage {
  role: string;
  content: string;
}

interface AnthropicRequest {
  messages: ChatMessage[];
  model: string;
}

interface OpenAIChunk {
  id: string;
  object: string;
  created: number;
  model: string;
  choices: Array<{
    index: number;
    delta: {
      content?: string;
    };
    finish_reason?: string | null;
  }>;
}

const app = express();
const PORT = parseInt(process.env.PORT || '4001', 10); // Different from main proxy port 4000
const HOST = process.env.HOST || '0.0.0.0'; // Bind to all interfaces for Docker

app.use(express.json({ limit: '50mb' }));

// Health check endpoint
app.get('/health', (req, res) => {
  res.json({ status: 'ok', service: 'anthropic-bridge' });
});

// Main chat endpoint
app.post('/anthropic/chat', async (req, res) => {
  const { messages, model }: AnthropicRequest = req.body;

  if (!messages || !Array.isArray(messages)) {
    return res.status(400).json({ error: 'Invalid messages format' });
  }

  console.log(`[Bridge] New request for model: ${model}, messages: ${messages.length}`);

  // Convert messages to prompt string
  const prompt = messages
    .map(msg => `${msg.role}: ${msg.content}`)
    .join('\n\n') + '\n\nAssistant:';

  console.log(`[Bridge] Prompt length: ${prompt.length} chars`);

  // Set SSE headers
  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache');
  res.setHeader('Connection', 'keep-alive');

  // Spawn Claude CLI process
  const claude = spawn('claude', ['-p', prompt], {
    env: { ...process.env, CI: 'true' } // Disable fancy spinner
  });

  let buffer = '';
  let hasStarted = false;

  // Handle stdout
  claude.stdout.on('data', (chunk: Buffer) => {
    const text = chunk.toString();
    const cleanText = stripAnsi(text);

    // Skip initial output that might contain prompts or UI elements
    if (!hasStarted) {
      // Look for actual response content (usually starts after some setup text)
      if (cleanText.includes('Assistant:') || cleanText.trim().length > 0) {
        hasStarted = true;
        // Extract content after "Assistant:" if present
        const content = cleanText.includes('Assistant:')
          ? cleanText.split('Assistant:').pop()?.trim() || ''
          : cleanText.trim();

        if (content) {
          buffer += content;
          sendChunk(content);
        }
      }
      return;
    }

    // Normal streaming content
    buffer += cleanText;
    sendChunk(cleanText);
  });

  // Handle stderr (CLI errors)
  claude.stderr.on('data', (data: Buffer) => {
    const errorText = data.toString();
    console.error(`[CLI Error] ${errorText}`);

    // Send error as SSE
    const errorChunk: OpenAIChunk = {
      id: 'chatcmpl-bridge-error',
      object: 'chat.completion.chunk',
      created: Math.floor(Date.now() / 1000),
      model: model || 'claude-3-5-sonnet',
      choices: [{
        index: 0,
        delta: { content: `[Error: ${errorText}]` },
        finish_reason: 'error'
      }]
    };

    res.write(`data: ${JSON.stringify(errorChunk)}\n\n`);
  });

  // Handle process completion
  claude.on('close', (code: number | null) => {
    console.log(`[Bridge] Claude process exited with code ${code}`);

    // Treat as error if: non-zero exit code OR signal termination (code === null)
    if (code !== 0 || code === null) {
      // Send error finish reason
      const errorChunk: OpenAIChunk = {
        id: 'chatcmpl-bridge-error',
        object: 'chat.completion.chunk',
        created: Math.floor(Date.now() / 1000),
        model: model || 'claude-3-5-sonnet',
        choices: [{
          index: 0,
          delta: {},
          finish_reason: 'error'
        }]
      };
      res.write(`data: ${JSON.stringify(errorChunk)}\n\n`);
    } else {
      // Send normal completion
      const finishChunk: OpenAIChunk = {
        id: 'chatcmpl-bridge-done',
        object: 'chat.completion.chunk',
        created: Math.floor(Date.now() / 1000),
        model: model || 'claude-3-5-sonnet',
        choices: [{
          index: 0,
          delta: {},
          finish_reason: 'stop'
        }]
      };
      res.write(`data: ${JSON.stringify(finishChunk)}\n\n`);
    }

    // Send termination signal
    res.write('data: [DONE]\n\n');
    res.end();
  });

  // Handle process errors
  claude.on('error', (error: Error) => {
    console.error(`[Bridge] Failed to spawn claude process: ${error.message}`);

    const errorChunk: OpenAIChunk = {
      id: 'chatcmpl-bridge-spawn-error',
      object: 'chat.completion.chunk',
      created: Math.floor(Date.now() / 1000),
      model: model || 'claude-3-5-sonnet',
      choices: [{
        index: 0,
        delta: { content: `[Spawn Error: ${error.message}]` },
        finish_reason: 'error'
      }]
    };

    res.write(`data: ${JSON.stringify(errorChunk)}\n\n`);
    res.write('data: [DONE]\n\n');
    res.end();
  });

  function sendChunk(content: string) {
    if (!content.trim()) return;

    const chunk: OpenAIChunk = {
      id: 'chatcmpl-bridge-stream',
      object: 'chat.completion.chunk',
      created: Math.floor(Date.now() / 1000),
      model: model || 'claude-3-5-sonnet',
      choices: [{
        index: 0,
        delta: { content },
        finish_reason: null
      }]
    };

    res.write(`data: ${JSON.stringify(chunk)}\n\n`);
  }
});

// Start server
app.listen(PORT, HOST, () => {
  console.log(`
==================================================
   ANTHROPIC CLI BRIDGE - ${HOST}:${PORT}
   TARGET: local "claude" CLI command
   MODE: STDIO-TO-HTTP BRIDGE
==================================================
1. Ensure you ran 'claude login' in your terminal
2. The Rust proxy should connect to http://${HOST}:${PORT}/anthropic/chat
3. Bridge serves OpenAI-compatible SSE responses
==================================================
  `);
});
