/**
 * API Contract: OpenAI Chat Completions (Supported Subset)
 * 
 * This defines the interface between the Client (Cursor) and the Proxy.
 * The Proxy must accept this format and translate it to the Provider's format.
 */

export type Role = 'system' | 'user' | 'assistant' | 'tool';

export interface ChatMessage {
    role: Role;
    content: string | Array<{ type: 'text' | 'image_url';[key: string]: any }>;
    name?: string;
    tool_calls?: any[];
}

export interface ChatCompletionRequest {
    model: string;
    messages: ChatMessage[];
    temperature?: number;
    top_p?: number;
    n?: number;
    stream?: boolean;
    stop?: string | string[];
    max_tokens?: number;
    presence_penalty?: number;
    frequency_penalty?: number;
    logit_bias?: Record<string, number>;
    user?: string;

    // Extended options for specific providers (passed through if supported)
    response_format?: { type: 'text' | 'json_object' };
}

export interface ChatCompletionChoice {
    index: number;
    message: ChatMessage;
    finish_reason: 'stop' | 'length' | 'content_filter' | 'tool_calls' | null;
}

export interface ChatCompletionResponse {
    id: string;
    object: 'chat.completion';
    created: number;
    model: string;
    choices: ChatCompletionChoice[];
    usage?: {
        prompt_tokens: number;
        completion_tokens: number;
        total_tokens: number;
    };
}

export interface ChatCompletionChunkChoice {
    index: number;
    delta: Partial<ChatMessage>;
    finish_reason: 'stop' | 'length' | 'content_filter' | 'tool_calls' | null;
}

export interface ChatCompletionChunk {
    id: string;
    object: 'chat.completion.chunk';
    created: number;
    model: string;
    choices: ChatCompletionChunkChoice[];
}

// Provider-Specific Types (Internal Translation Targets)

export interface VertexGenerateContentRequest {
    contents: Array<{
        role: string;
        parts: Array<{ text?: string; inlineData?: any }>;
    }>;
    generationConfig?: {
        temperature?: number;
        maxOutputTokens?: number;
        topP?: number;
        stopSequences?: string[];
    };
    safetySettings?: any[];
}
