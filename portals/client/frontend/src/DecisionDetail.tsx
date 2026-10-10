import { Badge, Code, Group, Stack, Text, Title } from "@mantine/core";
import { graphql, useFragment, useLazyLoadQuery } from "react-relay";

import type { DecisionDetail_decision$key } from "./__generated__/DecisionDetail_decision.graphql";
import type { DecisionDetailQuery } from "./__generated__/DecisionDetailQuery.graphql";
import { entityLabel, formatTime, prettyJson, VERDICT_COLOR, verdictLabel } from "./format";

/** Explains one decision: what was asked, the verdict, and the policies that produced it. */
export function DecisionDetail({ id }: { id: string }) {
  const { node } = useLazyLoadQuery<DecisionDetailQuery>(
    graphql`
      query DecisionDetailQuery($id: ID!) {
        node(id: $id) {
          ...DecisionDetail_decision @alias
        }
      }
    `,
    { id },
  );
  const decision = node?.DecisionDetail_decision;
  if (!decision) {
    return <Text c="dimmed">This decision is no longer in the log.</Text>;
  }
  return <DecisionView decision={decision} />;
}

function DecisionView({ decision }: { decision: DecisionDetail_decision$key }) {
  const d = useFragment(
    graphql`
      fragment DecisionDetail_decision on Decision {
        id
        at
        subject {
          type
          id
        }
        onBehalfOf {
          person {
            type
            id
          }
          verdict
          determiningPolicies {
            id
            source
          }
        }
        humanPresent
        action
        resource {
          type
          id
        }
        verdict
        mode
        challenges
        policyVersion
        context
        determiningPolicies {
          id
          source
        }
      }
    `,
    decision,
  );

  return (
    <Stack gap="md">
      <Group gap="sm">
        <Badge
          size="lg"
          color={VERDICT_COLOR[d.verdict as keyof typeof VERDICT_COLOR]}
          variant={d.mode === "MONITOR" ? "outline" : "light"}
        >
          {verdictLabel(d.verdict, d.mode, d.challenges)}
        </Badge>
        <Text c="dimmed">{formatTime(d.at)}</Text>
      </Group>

      <Stack gap={4}>
        <Field label="Subject" value={entityLabel(d.subject)} />
        {d.onBehalfOf && <Field label="Acting for" value={entityLabel(d.onBehalfOf.person)} />}
        <Field label="Person present" value={d.humanPresent ? "yes" : "no"} />
        <Field label="Action" value={d.action} />
        <Field label="Resource" value={entityLabel(d.resource)} />
        <Field
          label="Policy bundle"
          value={d.policyVersion === 0 ? "built-in defaults" : `version ${d.policyVersion}`}
        />
        <Field label="Mode" value={d.mode === "MONITOR" ? "monitor: recorded, not applied" : "enforce: applied"} />
        <Field label="Decision ID" value={d.id} />
      </Stack>

      <Stack gap={4}>
        <Title order={5}>Context</Title>
        <Code block>{prettyJson(d.context)}</Code>
      </Stack>

      <Stack gap={4}>
        <Title order={5}>Why{d.onBehalfOf ? ` (${entityLabel(d.subject)})` : ""}</Title>
        <PolicyList policies={d.determiningPolicies} />
      </Stack>

      {d.onBehalfOf && (
        <Stack gap={4}>
          <Group gap="sm">
            <Title order={5}>Could {entityLabel(d.onBehalfOf.person)} do this?</Title>
            <Badge color={VERDICT_COLOR[d.onBehalfOf.verdict as keyof typeof VERDICT_COLOR]} variant="light">
              {d.onBehalfOf.verdict.toLowerCase()}
            </Badge>
          </Group>
          <Text size="sm" c="dimmed">
            A service account acting for someone gets at most what they could do, so the stricter of the two checks
            decides.
          </Text>
          <PolicyList policies={d.onBehalfOf.determiningPolicies} />
        </Stack>
      )}
    </Stack>
  );
}

function PolicyList({ policies }: { policies: ReadonlyArray<{ readonly id: string; readonly source: string }> }) {
  if (policies.length === 0) {
    return <Text size="sm">No policy permitted this action, so it was denied by default.</Text>;
  }
  return (
    <>
      {policies.map((p) => (
        <Stack key={p.id} gap={2}>
          <Text size="sm" fw={600} ff="monospace">
            {p.id}
          </Text>
          <Code block>{p.source}</Code>
        </Stack>
      ))}
    </>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <Group gap="xs" wrap="nowrap">
      <Text size="sm" c="dimmed" w={110}>
        {label}
      </Text>
      <Text size="sm" ff="monospace" style={{ wordBreak: "break-all" }}>
        {value}
      </Text>
    </Group>
  );
}
