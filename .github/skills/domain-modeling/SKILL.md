---
name: domain-modeling
description: Build and sharpen a project's domain model. Use when discussing codebase terminology, writing or editing a GLOSSARY.md, or recording or editing an ADR.
---

# Domain Modeling

Actively build and sharpen the project's domain model while designing: challenge terminology, invent edge-case scenarios, and write down the glossary and decisions when they crystallize. This is for changing the model, not merely consulting an existing glossary.

## File structure

Most repositories have one context, with a root `GLOSSARY.md` and ADRs in `docs/adr/` (for example, `0001-short-title.md`). If `GLOSSARY-MAP.md` exists at the root, treat the repository as multi-context: use the map to locate context glossaries and context-specific ADRs; keep system-wide ADRs in root `docs/adr/`.

Create files lazily: create a glossary when the first term is resolved, and create `docs/adr/` when the first ADR is needed. Do not add empty or speculative documentation.

## During the session

### Challenge and sharpen language

When the user uses a term that conflicts with the glossary, point out the conflict and ask which meaning is intended. For vague or overloaded words, propose a precise canonical term and distinguish the concepts.

### Discuss concrete scenarios

Stress-test domain relationships with specific scenarios, especially edge cases that clarify boundaries between concepts.

### Cross-reference code

When the user explains behavior, check whether the implementation agrees. Surface contradictions and ask which understanding is correct rather than silently choosing one.

### Update the glossary inline

When a term is resolved, update `GLOSSARY.md` at that point using [GLOSSARY-FORMAT.md](./GLOSSARY-FORMAT.md). Keep it about domain language only: no implementation details, specs, scratch notes, or implementation decisions.

### Offer ADRs sparingly

Offer an ADR only when all three conditions hold: the decision is hard to reverse; a future reader would find it surprising without context; and it resulted from a real trade-off among alternatives. Use [ADR-FORMAT.md](./ADR-FORMAT.md). Skip ADRs for easy-to-reverse, obvious, or no-alternative choices.
