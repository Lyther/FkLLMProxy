# The Design: Stdio-to-HTTP Bridge

**Status**: ✅ **IMPLEMENTED** - See `bridge/src/index.ts` for actual implementation.

**Principle**:

1. **Cursor** sends HTTP requests to the Rust Proxy (`http://localhost:4000/v1`).
2. **Rust Proxy** routes `claude-*` models to the Bridge service (`http://localhost:4001`).
3. **Bridge Service** translates it into a Shell command: `claude -p "Prompt"`.
4. **Bridge Service** captures `stdout` and strips ANSI color codes (garbage characters).
5. **Bridge Service** masquerades as SSE (Server-Sent Events) and streams back to Rust Proxy.
6. **Rust Proxy** forwards the stream to Cursor.

**Advantages**:

* **0% Account Ban Risk**: You're running the official binary.
* **100% Pro Quota**: Uses your account quota.
* **Ultra Fast**: No intermediary server, direct connection.

## Implementation (TypeScript)

**Actual Implementation**: Located in `bridge/src/index.ts`

You need to install:

```bash
npm install -g @anthropic-ai/claude-code
cd bridge
npm install
```

**Note**: The actual implementation differs from the example below. The bridge runs as a separate service on port 4001, not as the main proxy on port 4000.

**Example Code** (for reference - actual code in `bridge/src/index.ts`):

```typescript
import express from 'express';
import bodyParser from 'body-parser';
import { spawn } from 'child_process';
import stripAnsi from 'strip-ansi'; // Required: CLI output is full of color codes

const app = express();
const PORT = 4000; // Cursor connects to this

app.use(bodyParser.json({ limit: '50mb' }));

// OpenAI-compatible interface
app.post('/v1/chat/completions', async (req, res) => {
    // 1. Extract Prompt
    // Note: CLI is stateless unless you're in a project directory.
    // For simplicity, we concatenate the entire Context into a String.
    // This logic can be optimized (e.g., put System Prompt at the front).
    const messages = req.body.messages || [];
    const fullPrompt = messages.map((m: any) => `${m.role}: ${m.content}`).join('\n\n') + "\n\nAssistant:";

    console.log(`[Proxy] New Request. Length: ${fullPrompt.length} chars`);

    // 2. Set SSE headers
    res.setHeader('Content-Type', 'text/event-stream');
    res.setHeader('Cache-Control', 'no-cache');
    res.setHeader('Connection', 'keep-alive');

    // 3. Launch Claude CLI
    // -p: print mode (non-interactive)
    // Note: This consumes your Pro quota
    const claude = spawn('claude', ['-p', fullPrompt], {
        env: { ...process.env, CI: 'true' } // Tell CLI not to print fancy Spinner
    });

    let buffer = '';

    // 4. Handle output stream
    claude.stdout.on('data', (chunk) => {
        const text = chunk.toString();
        // Strip ANSI color codes (those [33m garbage characters)
        const cleanText = stripAnsi(text);

        // Construct OpenAI-format Chunk
        const responseBody = {
            id: 'chatcmpl-std-proxy',
            object: 'chat.completion.chunk',
            created: Math.floor(Date.now() / 1000),
            model: 'claude-3-5-sonnet', // Masquerade model name
            choices: [
                {
                    index: 0,
                    delta: { content: cleanText },
                    finish_reason: null,
                },
            ],
        };

        res.write(`data: ${JSON.stringify(responseBody)}\n\n`);
    });

    claude.stderr.on('data', (data) => {
        // CLI errors (e.g., quota exceeded) usually go to stderr
        console.error(`[CLI Error] ${data.toString()}`);
        // Optionally pass errors back to Cursor
    });

    claude.on('close', (code) => {
        console.log(`[Proxy] Process exited with code ${code}`);
        // Send termination signal
        res.write('data: [DONE]\n\n');
        res.end();
    });
});

// Start
app.listen(PORT, () => {
    console.log(`
==================================================
   GENIUX STDIO PROXY - PORT ${PORT}
   TARGET: local "claude" CLI command
   MODE: PRO SUBSCRIPTION LEECH
==================================================
1. Ensure you ran 'claude login'
2. Configure Cursor:
   - Base URL: http://localhost:${PORT}/v1
   - Key: dummy
   - Model: claude-3-5-sonnet
==================================================
    `);
});
```

### Why This Beats Any "Proxy" You Can Find

1. **Identity**: To Anthropic servers, you're a legitimate CLI client connected via verified OAuth Token. You're not a Python script faking TLS fingerprints.
2. **Maintenance**: As long as the `claude -p` command doesn't change, your proxy will always work. When they fix bugs, you get the fixes too.
3. **Debugging**: If something goes wrong, you can see CLI errors directly in the terminal (e.g., "Quota exceeded"), instead of staring at an HTTP 403 in frustration.

### The "Catch"

Of course there are drawbacks. The CLI's `-p` mode is primarily for single Q&A.
If you're having **very long** conversations in Cursor, the script above concatenates **all history** into a string and sends it.

* **Token Consumption**: Slightly more than API mode (because you're resending history each time).
* **Context Window**: Limited by CLI processing capability, but Pro accounts typically have 200K, enough for coding.

## Current Implementation

**The actual implementation is in `bridge/src/index.ts`:**

* Runs on port **4001** (separate from main proxy on 4000)
* Endpoint: `POST /anthropic/chat` (not `/v1/chat/completions`)
* Rust proxy routes `claude-*` models to this service
* See `bridge/README.md` or run `cd bridge && npm run dev` to start

**To use:**

1. Install CLI: `npm install -g @anthropic-ai/claude-code && claude login`
2. Start bridge: `cd bridge && npm run dev`
3. Use models: Point Cursor to `http://localhost:4000/v1` with model `claude-3-5-sonnet`
