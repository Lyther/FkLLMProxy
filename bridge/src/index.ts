// bridge/src/index.ts
import { spawn } from 'child_process';
import express from 'express';
import pino from 'pino';
import stripAnsi from 'strip-ansi';

const logger = pino({
  level: process.env.LOG_LEVEL || 'info',
  ...(process.env.NODE_ENV !== 'production' && {
    transport: {
      target: 'pino-pretty',
      options: { translateTime: 'HH:MM:ss Z', ignore: 'pid,hostname' },
    },
  }),
});

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
    delta: { content?: string };
    finish_reason?: string | null;
  }>;
}

// Security constants
const MAX_PROMPT_LENGTH = 100000;
const MAX_MESSAGE_COUNT = 1000;
const MAX_ROLE_LENGTH = 20;
const MAX_CONTENT_LENGTH = 50000;
const ALLOWED_ROLES = new Set(['user', 'assistant', 'system', 'human', 'ai']);
const RESPONSE_DETECTION_THRESHOLD = 50;

// Validation helpers
function isValidRole(role: unknown): role is string {
  return (
    typeof role === 'string' &&
    role.length > 0 &&
    role.length <= MAX_ROLE_LENGTH &&
    ALLOWED_ROLES.has(role.toLowerCase())
  );
}

function isValidContent(content: unknown): content is string {
  return (
    typeof content === 'string' &&
    content.length > 0 &&
    content.length <= MAX_CONTENT_LENGTH
  );
}

function sanitizePrompt(prompt: string): string {
  // Remove null bytes and control characters (except newlines/tabs)
  let sanitized = prompt
    .replace(/\x00/g, '')
    .replace(/[\x01-\x08\x0B\x0C\x0E-\x1F\x7F]/g, '')
    .replace(/\\/g, '\\\\')
    .replace(/'/g, "\\'")
    .replace(/"/g, '\\"');

  if (sanitized.length > MAX_PROMPT_LENGTH) {
    throw new Error(`Prompt too long. Maximum ${MAX_PROMPT_LENGTH} characters allowed`);
  }

  return sanitized;
}

function detectAssistantResponse(
  text: string,
  preStartBuffer: string
): { hasStarted: boolean; content: string } {
  const trimmed = preStartBuffer.trim();

  if (text.includes('Assistant:')) {
    const content = text.split('Assistant:').pop()?.trim() || '';
    return { hasStarted: true, content };
  }

  if (
    trimmed.length > RESPONSE_DETECTION_THRESHOLD &&
    !trimmed.toLowerCase().includes('loading') &&
    !trimmed.toLowerCase().includes('please wait')
  ) {
    return { hasStarted: true, content: trimmed };
  }

  return { hasStarted: false, content: '' };
}

const app = express();
const PORT = parseInt(process.env.PORT || '4001', 10);
const HOST = process.env.HOST || '0.0.0.0';

// Security: Limit body size
app.use(express.json({ limit: '10mb' }));

// Security: Disable x-powered-by header
app.disable('x-powered-by');

app.get('/health', (_req, res) => {
  res.json({ status: 'ok', service: 'anthropic-bridge' });
});

app.post('/anthropic/chat', async (req, res) => {
  const { messages, model }: AnthropicRequest = req.body;

  // Validate messages array
  if (!messages || !Array.isArray(messages)) {
    logger.warn('Invalid request: messages is not an array');
    return res.status(400).json({ error: 'Invalid messages format' });
  }

  if (messages.length === 0) {
    logger.warn('Invalid request: empty messages array');
    return res.status(400).json({ error: 'Messages array cannot be empty' });
  }

  if (messages.length > MAX_MESSAGE_COUNT) {
    logger.warn({ count: messages.length }, 'Invalid request: too many messages');
    return res
      .status(400)
      .json({ error: `Too many messages. Maximum ${MAX_MESSAGE_COUNT} allowed` });
  }

  // Validate model if provided
  if (model !== undefined && typeof model !== 'string') {
    logger.warn('Invalid request: model is not a string');
    return res.status(400).json({ error: 'Model must be a string' });
  }

  let prompt: string;
  try {
    const rawPrompt =
      messages
        .map((msg, idx) => {
          if (!isValidRole(msg.role)) {
            throw new Error(
              `Invalid role at message ${idx}: must be one of ${Array.from(ALLOWED_ROLES).join(', ')}`
            );
          }
          if (!isValidContent(msg.content)) {
            throw new Error(
              `Invalid content at message ${idx}: must be non-empty string under ${MAX_CONTENT_LENGTH} chars`
            );
          }
          const sanitizedContent = sanitizePrompt(msg.content);
          return `${msg.role}: ${sanitizedContent}`;
        })
        .join('\n\n') + '\n\nAssistant:';

    prompt = sanitizePrompt(rawPrompt);
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    logger.warn({ err: error }, 'Input validation failed');
    return res.status(400).json({ error: `Invalid input: ${errorMessage}` });
  }

  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache, no-store, must-revalidate');
  res.setHeader('Connection', 'keep-alive');
  res.setHeader('X-Content-Type-Options', 'nosniff');

  const claude = spawn('claude', ['-p', prompt], {
    env: { ...process.env, CI: 'true' },
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: 300000, // 5 minute timeout
  });

  let buffer = '';
  let hasStarted = false;
  let preStartBuffer = '';

  claude.stdout.on('data', (chunk: Buffer) => {
    const text = chunk.toString();
    const cleanText = stripAnsi(text);

    if (!hasStarted) {
      const detection = detectAssistantResponse(cleanText, preStartBuffer);
      if (detection.hasStarted) {
        hasStarted = true;
        if (detection.content) {
          buffer += detection.content;
          sendChunk(detection.content);
        }
        preStartBuffer = '';
        return;
      }
      preStartBuffer += cleanText;
      return;
    }

    buffer += cleanText;
    sendChunk(cleanText);
  });

  claude.stderr.on('data', (data: Buffer) => {
    const errorText = data.toString();
    logger.error({ stderr: errorText }, 'CLI stderr output');

    const errorChunk: OpenAIChunk = {
      id: 'chatcmpl-bridge-error',
      object: 'chat.completion.chunk',
      created: Math.floor(Date.now() / 1000),
      model: model || 'claude-3-5-sonnet',
      choices: [
        {
          index: 0,
          delta: { content: `[Error: ${errorText}]` },
          finish_reason: 'error',
        },
      ],
    };

    res.write(`data: ${JSON.stringify(errorChunk)}\n\n`);
  });

  claude.on('close', (code: number | null) => {
    if (code !== 0 || code === null) {
      logger.warn({ exitCode: code }, 'Claude process exited with non-zero code');
      const errorChunk: OpenAIChunk = {
        id: 'chatcmpl-bridge-error',
        object: 'chat.completion.chunk',
        created: Math.floor(Date.now() / 1000),
        model: model || 'claude-3-5-sonnet',
        choices: [{ index: 0, delta: {}, finish_reason: 'error' }],
      };
      res.write(`data: ${JSON.stringify(errorChunk)}\n\n`);
    } else {
      const finishChunk: OpenAIChunk = {
        id: 'chatcmpl-bridge-done',
        object: 'chat.completion.chunk',
        created: Math.floor(Date.now() / 1000),
        model: model || 'claude-3-5-sonnet',
        choices: [{ index: 0, delta: {}, finish_reason: 'stop' }],
      };
      res.write(`data: ${JSON.stringify(finishChunk)}\n\n`);
    }

    res.write('data: [DONE]\n\n');
    res.end();
  });

  claude.on('error', (error: Error) => {
    logger.error({ err: error }, 'Failed to spawn claude process');

    const errorChunk: OpenAIChunk = {
      id: 'chatcmpl-bridge-spawn-error',
      object: 'chat.completion.chunk',
      created: Math.floor(Date.now() / 1000),
      model: model || 'claude-3-5-sonnet',
      choices: [
        {
          index: 0,
          delta: { content: `[Spawn Error: ${error.message}]` },
          finish_reason: 'error',
        },
      ],
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
      choices: [{ index: 0, delta: { content }, finish_reason: null }],
    };

    res.write(`data: ${JSON.stringify(chunk)}\n\n`);
  }
});

app.listen(PORT, HOST, () => {
  logger.info({ host: HOST, port: PORT }, 'Anthropic CLI Bridge started');
});
