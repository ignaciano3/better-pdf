---
title: For AI agents
description: better-pdf ships an agent skill and a typed workflow for correct automated use.
---

better-pdf ships an
[agent skill](https://github.com/ignaciano3/better-pdf/blob/master/skills/better-pdf/SKILL.md) —
procedural knowledge for driving the library correctly: the
load → inspect → generate types → fill/flatten/sign → save workflow, plus the
non-obvious rules:

- Use a field's *real* export values, never assume `Yes`/`On`.
- Visual signatures are not cryptographic.
- `save()` is an incremental update.

It installs into 20+ agents via [skills.sh](https://www.skills.sh).

## The strongest feature: typed forms

The strongest agent-readiness feature is the [typed workflow](/better-pdf/guides/typed-forms/):
generate a types module from the PDF and `doc.getForm<typeof myFormFields>()`
turns hallucinated field names and invalid values into compile errors.
