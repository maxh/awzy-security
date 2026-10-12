import { EmptyState, PageHeader, PageLoader, Section, Stat, StatusBadge } from "@awzy/ui";
import { Alert, Button, Group, Modal, SimpleGrid, Stack, Table, Text } from "@mantine/core";
import { getRouteApi, useRouter } from "@tanstack/react-router";
import { IconHistory, IconShieldCheck } from "@tabler/icons-react";
import { Suspense, useState } from "react";
import { graphql, type PreloadedQuery, usePreloadedQuery, useRelayEnvironment } from "react-relay";

import type { EnforcementPageQuery } from "./__generated__/EnforcementPageQuery.graphql";
import type { EnforcementPageSetModeMutation } from "./__generated__/EnforcementPageSetModeMutation.graphql";
import { Entity, formatTime, Time } from "./format";
import { OutageFallbackSection } from "./OutageFallbackSection";
import { ScopedEnforcementSection } from "./ScopedEnforcementSection";
import { mutate, passChallenge } from "./securityKeyChallenge";
import { webAuthnMessage } from "./webauthn";

const setMode = graphql`
  mutation EnforcementPageSetModeMutation($input: SetEnforcementModeInput!) {
    setEnforcementMode(input: $input) {
      enforcement {
        mode
        changedBy {
          label
        }
        changedAt
      }
      challengeQid
    }
  }
`;

const route = getRouteApi("/enforcement");

/**
 * Monitor or enforce for the whole install (ADR 0007), and what enforcing would change; the same
 * switch for awzy-reasoner's automated containments (ADR 0066); and modes for one action or tool
 * (ADR 0109).
 */
export function EnforcementPage() {
  const queryRef = route.useLoaderData();
  const router = useRouter();
  return (
    <>
      <PageHeader
        title="Enforcement"
        description="Whether decisions are enforced or only recorded, and what enforcing would have denied (ADR 0007)."
      />
      <Suspense fallback={<PageLoader />}>
        <Enforcement queryRef={queryRef} onChanged={() => void router.invalidate()} />
      </Suspense>
    </>
  );
}

function percent(part: number, total: number): string {
  return total === 0 ? "0%" : `${((part / total) * 100).toFixed(1)}%`;
}

function Enforcement({
  queryRef,
  onChanged,
}: {
  queryRef: PreloadedQuery<EnforcementPageQuery>;
  onChanged: () => void;
}) {
  const data = usePreloadedQuery<EnforcementPageQuery>(
    graphql`
      query EnforcementPageQuery {
        canSwitch: viewerCan(action: ADMINISTER, area: ENFORCEMENT)
        enforcement {
          mode
          changedBy {
            label
          }
          changedAt
        }
        containmentEnforcement: enforcement(scope: AUTOMATED_CONTAINMENTS) {
          mode
          changedBy {
            label
          }
          changedAt
        }
        containmentReview {
          automated
          liftedOnReview
        }
        enforcementSummary {
          since
          total
          monitored
          wouldDeny
          wouldChallenge
          enforcedDeny
          enforcedChallenge
        }
        wouldDeny: decisions(first: 5, filter: { mode: MONITOR, verdict: DENY }) {
          edges {
            node {
              id
              at
              principal {
                type
                id
              }
              action
              resource {
                type
                id
              }
              policies
            }
          }
        }
        enforcementHistory(first: 10) {
          mode
          changedBy {
            label
          }
          changedAt
        }
      }
    `,
    queryRef,
  );
  const environment = useRelayEnvironment();
  const [isSaving, setSaving] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [confirmingContainments, setConfirmingContainments] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const { enforcement: current, containmentEnforcement: containments, enforcementSummary: s } = data;
  const review = data.containmentReview;
  const enforcing = current.mode === "ENFORCE";
  const target = enforcing ? "MONITOR" : "ENFORCE";
  const containing = containments.mode === "ENFORCE";
  // Switching asks for the admin's security key (the base policies): the first try returns the
  // challenge, a touch of the key passes it, and the second try switches.
  const switchScope = async (scope: "INSTALL" | "AUTOMATED_CONTAINMENTS", mode: "MONITOR" | "ENFORCE") => {
    setSaving(true);
    try {
      const input = { input: { mode, scope } };
      let { setEnforcementMode: res } = await mutate<EnforcementPageSetModeMutation>(environment, setMode, input);
      if (res.challengeQid) {
        await passChallenge(environment, res.challengeQid);
        ({ setEnforcementMode: res } = await mutate<EnforcementPageSetModeMutation>(environment, setMode, input));
      }
      if (!res.enforcement) throw new Error("awzy still asks for a challenge before switching; try again.");
      setConfirming(false);
      setConfirmingContainments(false);
      setError(null);
      onChanged();
    } catch (e) {
      setError(webAuthnMessage(e));
    } finally {
      setSaving(false);
    }
  };
  const switchMode = () => void switchScope("INSTALL", target);

  return (
    <Stack gap="lg">
      <Section
        title="Mode"
        actions={
          data.canSwitch ? (
            <Button color={enforcing ? "blue" : "red"} onClick={() => setConfirming(true)}>
              {enforcing ? "Switch back to monitor" : "Switch to enforce"}
            </Button>
          ) : undefined
        }
      >
        <Stack gap="xs">
          <Group gap="sm">
            <StatusBadge tone={enforcing ? "danger" : "info"}>{enforcing ? "Enforcing" : "Monitoring"}</StatusBadge>
            <Text size="sm" c="dimmed">
              {current.changedAt
                ? `since ${formatTime(current.changedAt)} by ${current.changedBy?.label ?? "unknown"}`
                : "New installs start in monitor mode and stay there until an admin switches them."}
            </Text>
          </Group>
          <Text size="sm" maw={760}>
            {enforcing
              ? "Enforcement points apply every verdict: denies block, challenges are sent."
              : "awzy decides every request and records the verdict, but enforcement points allow everything and send no challenges. Review what would have been denied before you enforce."}
          </Text>
          {!data.canSwitch && (
            <Text size="xs" c="dimmed">
              You can't switch modes. Policy decides who can, through the PDP.
            </Text>
          )}
        </Stack>
      </Section>

      <Section
        title="Automated containments"
        actions={
          data.canSwitch ? (
            <Button variant="light" color={containing ? "blue" : "red"} onClick={() => setConfirmingContainments(true)}>
              {containing ? "Switch back to monitor" : "Switch to enforce"}
            </Button>
          ) : undefined
        }
      >
        <Stack gap="xs">
          <Group gap="sm">
            <StatusBadge tone={containing ? "danger" : "info"}>{containing ? "Enforcing" : "Monitoring"}</StatusBadge>
            <Text size="sm" c="dimmed">
              {containments.changedAt
                ? `since ${formatTime(containments.changedAt)} by ${containments.changedBy?.label ?? "unknown"}`
                : "Starts in monitor mode, separately from the install's."}
            </Text>
          </Group>
          <Text size="sm" maw={760}>
            {containing
              ? "awzy-reasoner contains a person when their risk is elevated, a challenge for them was rejected, and they tried to move data out, all in the same day. Every PDP applies it within seconds."
              : "awzy-reasoner records whom it would contain (elevated risk, a rejected challenge, and an attempt to move data out, in the same day) without applying it. Review them on the Challenges page, where you can contain the person or dismiss the record."}
          </Text>
          {review && review.automated > 0 && (
            <Text size="sm" c="dimmed" maw={760}>
              Lifted on review: {review.liftedOnReview} of the {review.automated} containments awzy-reasoner made in the
              last 30 days ({Math.round((100 * review.liftedOnReview) / review.automated)}%), lifted or dismissed by a
              person. The lower it is, the more its containments hold up.
            </Text>
          )}
        </Stack>
      </Section>

      <Suspense fallback={<PageLoader />}>
        <ScopedEnforcementSection canSwitch={data.canSwitch} onChanged={onChanged} />
      </Suspense>

      <OutageFallbackSection canEdit={data.canSwitch} />

      <Modal
        opened={confirmingContainments}
        onClose={() => setConfirmingContainments(false)}
        title={<Text fw={600}>{containing ? "Stop automated containments?" : "Enforce automated containments?"}</Text>}
      >
        <Stack gap="sm">
          <Text size="sm">
            {containing
              ? "awzy-reasoner will only record whom it would contain. Containments already in force stay until someone lifts them."
              : "From its next run, awzy-reasoner's containments take effect: a contained person, and every agent acting for them, is denied until someone lifts it."}
          </Text>
          <Text size="sm" c="dimmed">
            You&apos;ll touch your security key to confirm. Add one on the Challenges page first if you haven&apos;t.
          </Text>
          {error && (
            <Alert color="red" title="Not switched">
              {error}
            </Alert>
          )}
          <Group justify="flex-end">
            <Button variant="default" onClick={() => setConfirmingContainments(false)}>
              Cancel
            </Button>
            <Button
              color={containing ? "blue" : "red"}
              loading={isSaving}
              onClick={() => void switchScope("AUTOMATED_CONTAINMENTS", containing ? "MONITOR" : "ENFORCE")}
            >
              {containing ? "Switch to monitor" : "Enforce"}
            </Button>
          </Group>
        </Stack>
      </Modal>

      <div>
        <Text size="sm" c="dimmed" mb="xs">
          Since {formatTime(s.since)}
        </Text>
        <SimpleGrid cols={{ base: 2, md: 5 }}>
          <Stat label="Decisions" value={s.total.toLocaleString()} />
          <Stat
            label="Would deny"
            value={s.wouldDeny.toLocaleString()}
            hint={`${percent(s.wouldDeny, s.monitored)} of monitored`}
          />
          <Stat
            label="Would challenge"
            value={s.wouldChallenge.toLocaleString()}
            hint={`${percent(s.wouldChallenge, s.monitored)} of monitored`}
          />
          <Stat label="Denied" value={s.enforcedDeny.toLocaleString()} hint="enforced" />
          <Stat label="Challenged" value={s.enforcedChallenge.toLocaleString()} hint="enforced" />
        </SimpleGrid>
      </div>

      {!enforcing && (
        <Section
          title="Recent would-deny decisions"
          description="The Decisions page lists all of them: filter by mode Monitored and verdict Deny."
          flush
        >
          {data.wouldDeny.edges.length === 0 ? (
            <EmptyState icon={<IconShieldCheck size={22} />} title="None in the last 24 hours" />
          ) : (
            <Table.ScrollContainer minWidth={720}>
              <Table style={{ whiteSpace: "nowrap" }}>
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>Time</Table.Th>
                    <Table.Th>Subject</Table.Th>
                    <Table.Th>Action</Table.Th>
                    <Table.Th>Resource</Table.Th>
                    <Table.Th>Policies</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {data.wouldDeny.edges.map(({ node: d }) => (
                    <Table.Tr key={d.id}>
                      <Table.Td>
                        <Time at={d.at} />
                      </Table.Td>
                      <Table.Td>
                        <Entity e={d.principal} />
                      </Table.Td>
                      <Table.Td>{d.action}</Table.Td>
                      <Table.Td>
                        <Entity e={d.resource} />
                      </Table.Td>
                      <Table.Td ff="monospace" fz="xs">
                        {d.policies.join(", ") || "denied by default"}
                      </Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </Table.ScrollContainer>
          )}
        </Section>
      )}

      <Modal
        opened={confirming}
        onClose={() => setConfirming(false)}
        title={<Text fw={600}>{enforcing ? "Switch back to monitor?" : "Start enforcing?"}</Text>}
      >
        <Stack gap="sm">
          <Text size="sm">
            {enforcing
              ? "Enforcement points will allow every request again and stop sending challenges. Decisions are still recorded."
              : `Enforcement points will start applying verdicts within seconds. At the last 24 hours' rate, ${percent(
                  s.wouldDeny,
                  s.monitored,
                )} of requests would be denied and ${percent(s.wouldChallenge, s.monitored)} challenged.`}
          </Text>
          <Text size="sm" c="dimmed">
            You&apos;ll touch your security key to confirm. Add one on the Challenges page first if you haven&apos;t.
          </Text>
          {error && (
            <Alert color="red" title="Not switched">
              {error}
            </Alert>
          )}
          <Group justify="flex-end">
            <Button variant="default" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button color={enforcing ? "blue" : "red"} loading={isSaving} onClick={switchMode}>
              {enforcing ? "Switch to monitor" : "Enforce"}
            </Button>
          </Group>
        </Stack>
      </Modal>

      <Section title="History" flush>
        {data.enforcementHistory.length === 0 ? (
          <EmptyState icon={<IconHistory size={22} />} title="Never switched" />
        ) : (
          <Table>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>When</Table.Th>
                <Table.Th>Mode</Table.Th>
                <Table.Th>By</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {data.enforcementHistory.map((c) => (
                <Table.Tr key={`${c.changedAt ?? ""}/${c.mode}`}>
                  <Table.Td>{c.changedAt ? formatTime(c.changedAt) : ""}</Table.Td>
                  <Table.Td>
                    <StatusBadge tone={c.mode === "ENFORCE" ? "danger" : "info"}>
                      {c.mode === "ENFORCE" ? "Enforce" : "Monitor"}
                    </StatusBadge>
                  </Table.Td>
                  <Table.Td>{c.changedBy?.label}</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        )}
      </Section>
    </Stack>
  );
}
