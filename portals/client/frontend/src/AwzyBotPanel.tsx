import { Alert, Badge, Button, Group, Paper, ScrollArea, Stack, Text, Textarea } from "@mantine/core";
import { useState } from "react";
import { graphql, useMutation } from "react-relay";

import type { AwzyBotPanelAskMutation } from "./__generated__/AwzyBotPanelAskMutation.graphql";

type Turn =
  | { who: "you"; text: string }
  | { who: "bot"; text: string; drafts: number[]; tools: readonly string[]; refused: boolean }
  | { who: "error"; text: string };

const TOOL_LABELS: Record<string, string> = {
  search_decisions: "searched decisions",
  explain_decision: "explained a decision",
  get_active_policies: "read the active policies",
  draft_policy: "saved a draft",
};

/**
 * Awzy Bot (ADR 0048): explains decisions and drafts policy changes. It can't publish; a draft it
 * saves shows up in the bundle list for a person to review. `onDraftSaved` refreshes that list.
 */
export function AwzyBotPanel({ onDraftSaved }: { onDraftSaved: () => void }) {
  const [turns, setTurns] = useState<Turn[]>([]);
  // The conversation as the server last returned it; sent back unchanged with the next question.
  const [transcript, setTranscript] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const [commit, isAsking] = useMutation<AwzyBotPanelAskMutation>(graphql`
    mutation AwzyBotPanelAskMutation($input: AskAwzyBotInput!) {
      askAwzyBot(input: $input) {
        reply
        transcript
        toolsUsed
        refused
        drafts {
          id
          version
        }
      }
    }
  `);

  const ask = () => {
    const text = message.trim();
    if (!text || isAsking) return;
    setTurns((t) => [...t, { who: "you", text }]);
    setMessage("");
    commit({
      variables: { input: { message: text, transcript } },
      onCompleted: ({ askAwzyBot: res }, errors) => {
        if (errors?.length) {
          setTurns((t) => [...t, { who: "error", text: errors.map((e) => e.message).join("; ") }]);
          return;
        }
        setTranscript(res.transcript);
        const drafts = res.drafts.map((d) => d.version);
        setTurns((t) => [...t, { who: "bot", text: res.reply, drafts, tools: res.toolsUsed, refused: res.refused }]);
        if (drafts.length) onDraftSaved();
      },
      onError: (e) => setTurns((t) => [...t, { who: "error", text: e.message }]),
    });
  };

  const startOver = () => {
    setTurns([]);
    setTranscript(null);
  };

  return (
    <Stack gap="sm" h="100%">
      <Text size="sm" c="dimmed">
        Ask why something was allowed or denied, or describe a policy change. Awzy Bot can read decisions and policies
        you can see and save drafts; it can't publish them.
      </Text>
      <ScrollArea.Autosize mah="60vh" type="auto">
        <Stack gap="xs">
          {turns.length === 0 && (
            <Text size="sm" c="dimmed" fs="italic">
              e.g. "Why was bob denied the board deck today?" or "Draft a policy so contractors need approval for
              confidential documents."
            </Text>
          )}
          {turns.map((turn, i) => (
            <TurnView key={i} turn={turn} />
          ))}
          {isAsking && (
            <Text size="sm" c="dimmed">
              Awzy Bot is working…
            </Text>
          )}
        </Stack>
      </ScrollArea.Autosize>
      <Textarea
        aria-label="Ask Awzy Bot"
        placeholder="Ask Awzy Bot"
        autosize
        minRows={2}
        maxRows={8}
        value={message}
        onChange={(e) => setMessage(e.currentTarget.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            ask();
          }
        }}
      />
      <Group justify="space-between">
        <Button variant="subtle" size="xs" onClick={startOver} disabled={turns.length === 0 || isAsking}>
          New conversation
        </Button>
        <Button onClick={ask} loading={isAsking} disabled={!message.trim()}>
          Ask
        </Button>
      </Group>
    </Stack>
  );
}

function TurnView({ turn }: { turn: Turn }) {
  if (turn.who === "you") {
    return (
      <Paper p="xs" bg="var(--mantine-color-default-hover)" ml="xl">
        <Text size="sm" style={{ whiteSpace: "pre-wrap" }}>
          {turn.text}
        </Text>
      </Paper>
    );
  }
  if (turn.who === "error") {
    return (
      <Alert color="red" p="xs">
        <Text size="sm">{turn.text}</Text>
      </Alert>
    );
  }
  return (
    <Paper p="xs" withBorder mr="xl">
      <Stack gap={6}>
        <Text size="sm" style={{ whiteSpace: "pre-wrap" }} c={turn.refused ? "dimmed" : undefined}>
          {turn.text}
        </Text>
        {turn.tools.length > 0 && (
          <Group gap={4}>
            {[...new Set(turn.tools)].map((t) => (
              <Badge key={t} size="xs" variant="light" color="gray">
                {TOOL_LABELS[t] ?? t}
              </Badge>
            ))}
          </Group>
        )}
        {turn.drafts.map((v) => (
          <Alert key={v} color="blue" p="xs" title={`Draft version ${v} saved`}>
            <Text size="sm">Review it in the bundle list; it isn't enforced until someone publishes it.</Text>
          </Alert>
        ))}
      </Stack>
    </Paper>
  );
}
