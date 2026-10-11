---
title: Employees
description: What working with awzy is like for everyone else, and what to do when a request is challenged.
---

Most people never see awzy. It checks what they and their AI agents do, and allows what policy
allows. They notice it only when a request is challenged or denied.

## When a request is challenged

A challenge means "yes, once you've done this". The agent's tool call comes back with a message
saying what's needed, such as:

> awzy requires justification and approval for read on the FY27 strategic plan. Challenge
> challenge_0199… is waiting for an answer; once it's passed, retry the same call.

What to do:

1. **Open the challenge.** It's on the client portal's **Challenges** page, under *Waiting for
   you*. If your company has [Slack connected](/guides/optional-features/#slack-notifications), a
   direct message links to it.
2. **Answer it:**
   - **Justification:** say why you need the access. It's recorded with the request, for the
     security team to review.
   - **Security key:** touch your security key. No agent can do this for you, which is the point:
     it proves a person is there.
3. **Wait for approval, if it asks for one.** Someone else, such as the data's owner or the
   security team, approves or rejects it. With Slack connected, you're told which.
4. **Retry.** Ask your agent to make the same request again. A passed challenge holds for a short
   while, for that exact request.

A rejected request stays denied for a day. Talk to the person who rejected it, or to your security
team.

## When a request is denied

A deny means no policy allows it. The message includes the decision's ID. If you need the access,
give that ID to your security team: they can see exactly which policy decided it, and why.

If everything you do is suddenly denied, your access may be **contained** while the security team
reviews something. Contact them.

## Your security keys

The first time a challenge asks for your security key, the Challenges page has you add one. Your
first key needs only your sign-in; adding or removing one later needs a key you already have. If
you lose your key, ask your security team to reset it, then add a new one.

## Working through AI agents

When an agent acts for you, awzy checks both the agent and you, and the stricter answer wins. An
agent never gets more than both of you could. Challenges for an agent's requests come to you,
because you're the person it's acting for.

If you run an agent with your own sign-in, such as a coding agent using your token, awzy sees it
as you: what it does counts as yours, including toward your risk score. Don't let it answer
challenges for you, and keep your security key to yourself: touching it is how awzy knows it's
really you. See
[People, agents, and delegation](/concepts/principals-and-delegation/).
