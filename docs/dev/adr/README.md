# Architecture Decision Records (ADRs)

This directory contains Architecture Decision Records (ADRs) documenting significant architectural decisions made in the FkLLMProxy project.

## What are ADRs?

ADRs document the context, decision, and consequences of significant architectural choices. They serve as a historical record and help team members understand why certain decisions were made.

## Format

Each ADR follows this structure:

- **Status**: Accepted / Deprecated / Superseded
- **Context**: Why are we making this decision?
- **Decision**: What did we choose?
- **Consequences**: What are the trade-offs and implications?

## ADR Index

No ADRs have been created yet. When making a significant architectural decision, create a new ADR following the format:

```text
docs/adr/00X-short-descriptive-name.md
```

## When to Create an ADR

Create an ADR when making decisions about:

- Technology choices (e.g., "Why we chose Axum over Actix")
- Architecture patterns (e.g., "Why we use a provider abstraction pattern")
- Data formats (e.g., "Why we transform OpenAI format instead of using native formats")
- Infrastructure choices (e.g., "Why we use Docker Compose for deployment")

## References

- [Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) - Original ADR format
- [ADR Tools](https://adr.github.io/) - ADR tooling and templates
