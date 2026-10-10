---
title: People, agents, and delegation
description: Humans, service accounts, and what an agent may do for a person.
---

awzy has two kinds of accessor:

- **`Human`**: a person, identified by your IdP. Their groups and whether they're still active
  come from your IdP over SCIM.
- **`ServiceAccount`**: anything else with its own credentials: a backend service, a scheduled
  job, or an AI agent. Each is registered in awzy-context with an accountable owner, and marked
  as AI or not. A service account awzy doesn't know is denied before any policy runs.

## Acting for someone

When an agent acts for a person (an RFC 8693 token exchange, with the person as `sub` and the
agent as `act`), awzy checks twice: once as the agent, once as the person. The stricter verdict
wins, so **an agent never gets more than the person it acts for**.

Whether a person is there to answer a challenge is part of the request (`human_present`). An
agent working while its person is away can't be verified in the moment, so policy can ask for an
approval instead.
