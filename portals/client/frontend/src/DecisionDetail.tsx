import { Code, Divider, Group, Stack, Text } from "@mantine/core";
import type { ReactNode } from "react";
import { graphql, useFragment, useLazyLoadQuery } from "react-relay";

import type { DecisionDetail_decision$key } from "./__generated__/DecisionDetail_decision.graphql";
import type { DecisionDetailQuery } from "./__generated__/DecisionDetailQuery.graphql";
import { entityLabel, formatTime, Hidden, prettyObject, VerdictBadge } from "./format";

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
        principal {
          type
          id
        }
        onBehalfOf {
          human {
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
        <VerdictBadge verdict={d.verdict} mode={d.mode} challenges={d.challenges} />
        <Text size="sm" c="dimmed">
          {formatTime(d.at)}
        </Text>
      </Group>

      <Stack gap={6}>
        <Field label="Principal" value={entityLabel(d.principal)} />
        {d.onBehalfOf && <Field label="Acting for" value={entityLabel(d.onBehalfOf.human)} />}
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

      <Divider />
      <Stack gap={6}>
        <Heading>Context</Heading>
        {d.context == null ? <Hidden /> : <Code block>{prettyObject(d.context)}</Code>}
      </Stack>

      <Stack gap={6}>
        <Heading>Why{d.onBehalfOf ? ` (${entityLabel(d.principal)})` : ""}</Heading>
        <PolicyList policies={d.determiningPolicies} />
      </Stack>

      {d.onBehalfOf && (
        <Stack gap={6}>
          <Group gap="sm">
            <Heading>Could {entityLabel(d.onBehalfOf.human)} do this?</Heading>
            <VerdictBadge verdict={d.onBehalfOf.verdict} mode="ENFORCE" />
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

function Heading({ children }: { children: ReactNode }) {
  return (
    <Text size="xs" fw={600} tt="uppercase" c="dimmed" style={{ letterSpacing: "0.04em" }}>
      {children}
    </Text>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <Group gap="xs" wrap="nowrap">
      <Text size="sm" c="dimmed" w={120} style={{ flexShrink: 0 }}>
        {label}
      </Text>
      <Text size="sm" ff="monospace" style={{ wordBreak: "break-all" }}>
        {value}
      </Text>
    </Group>
  );
}
