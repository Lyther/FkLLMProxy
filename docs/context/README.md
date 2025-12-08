# Context Dump Directory

**PURPOSE**: Raw context injection for AI-assisted development.

## How to Use

**DUMP CONTEXT HERE**. Paste raw requirements, emails, brain dumps, or any unstructured information that defines the project's intent and constraints.

The AI reads this folder to understand:

- Business requirements
- Technical constraints
- User stories
- Design decisions
- Known issues
- Future plans

## File Naming Convention

- `kebab-case.md` only
- `requirements.md` - Functional requirements
- `constraints.md` - Technical/non-functional requirements
- `stakeholders.md` - User personas and needs
- `decisions.md` - Architecture decisions
- `issues.md` - Known bugs or blockers
- `roadmap.md` - Future development plans

## Example Content

```text
# Requirements

User needs to proxy LLM requests to multiple providers:
- OpenAI
- Anthropic
- Google Vertex AI

Must support:
- Streaming responses
- Rate limiting
- Authentication
- Error handling
```

Files in this directory are project assets and should be committed to version control.
