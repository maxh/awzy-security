import { routeQuery } from "@awzy/relay/routes";
import { PageHeader, PageLoader, Section, StatusBadge, type Tone } from "@awzy/ui";
import { Alert, Button, Drawer, Group, Modal, Stack, Table, Text, Textarea } from "@mantine/core";
import { getRouteApi, useRouter } from "@tanstack/react-router";
import { IconSparkles } from "@tabler/icons-react";
import { Suspense, useState } from "react";
import { graphql, type PreloadedQuery, useLazyLoadQuery, useMutation, usePreloadedQuery } from "react-relay";

import { AwzyBotPanel } from "./AwzyBotPanel";
import { DryRunReport } from "./DryRunReport";
import type { PoliciesPageCreateDraftMutation } from "./__generated__/PoliciesPageCreateDraftMutation.graphql";
import type { PoliciesPagePublishMutation } from "./__generated__/PoliciesPagePublishMutation.graphql";
import type { PoliciesPageAwzyBotQuery } from "./__generated__/PoliciesPageAwzyBotQuery.graphql";
import type { PoliciesPageQuery } from "./__generated__/PoliciesPageQuery.graphql";
import PoliciesPageQueryNode from "./__generated__/PoliciesPageQuery.graphql";
import { formatTime } from "./format";

/** Started by the route's loader as navigation begins (ADR 0099). */
export const loadPoliciesPage = routeQuery<PoliciesPageQuery>(PoliciesPageQueryNode);

const STATUS: Record<string, { label: string; tone: Tone }> = {
  ACTIVE: { label: "Active", tone: "success" },
  DRAFT: { label: "Draft", tone: "info" },
  SUPERSEDED: { label: "Superseded", tone: "neutral" },
};

const route = getRouteApi("/policies");

export function PoliciesPage() {
  const queryRef = route.useLoaderData();
  const router = useRouter();
  const [botOpen, setBotOpen] = useState(false);
  // After a mutation: the loader loads the bundle list again.
  const onChanged = () => void router.invalidate();
  return (
    <Stack gap="md">
      <PageHeader
        title="Policies"
        description="The Cedar policy bundles the PDPs enforce: drafts, dry runs, and what's published."
        actions={
          <Suspense fallback={null}>
            <AskAwzyBotButton onClick={() => setBotOpen(true)} />
          </Suspense>
        }
      />
      {/* Kept mounted while closed, so the conversation survives closing the drawer. */}
      <Drawer
        opened={botOpen}
        onClose={() => setBotOpen(false)}
        position="right"
        size="md"
        title="Awzy Bot"
        keepMounted
      >
        <AwzyBotPanel onDraftSaved={onChanged} />
      </Drawer>
      <Suspense fallback={<PageLoader />}>
        <Policies queryRef={queryRef} onChanged={onChanged} />
      </Suspense>
    </Stack>
  );
}

/** Shown only when the install has Awzy Bot set up (ADR 0048). */
function AskAwzyBotButton({ onClick }: { onClick: () => void }) {
  const { awzyBotEnabled } = useLazyLoadQuery<PoliciesPageAwzyBotQuery>(
    graphql`
      query PoliciesPageAwzyBotQuery {
        awzyBotEnabled
      }
    `,
    {},
  );
  return awzyBotEnabled ? (
    <Button variant="light" leftSection={<IconSparkles size={16} />} onClick={onClick}>
      Ask Awzy Bot
    </Button>
  ) : null;
}

function Policies({ queryRef, onChanged }: { queryRef: PreloadedQuery<PoliciesPageQuery>; onChanged: () => void }) {
  const data = usePreloadedQuery<PoliciesPageQuery>(
    graphql`
      query PoliciesPageQuery {
        canPublish: viewerCan(action: ADMINISTER, area: POLICIES)
        activePolicyBundle {
          id
          version
          schema
          policies
          publishedAt
          publishedBy {
            label
          }
        }
        policyBundles(first: 20) {
          id
          version
          status
          createdAt
          createdBy {
            label
          }
          publishedAt
        }
      }
    `,
    queryRef,
  );
  const [commitPublish, isPublishing] = useMutation<PoliciesPagePublishMutation>(graphql`
    mutation PoliciesPagePublishMutation($input: PublishPolicyBundleInput!) {
      publishPolicyBundle(input: $input) {
        bundle {
          id
          status
          publishedAt
        }
        wouldLockYouOut
      }
    }
  `);
  const [publishError, setPublishError] = useState<string | null>(null);
  // A draft that would stop the signed-in person administering policies, held until they accept it.
  const [lockout, setLockout] = useState<{ id: string; why: string } | null>(null);
  const [dryRun, setDryRun] = useState<{ id: string; version: number } | null>(null);
  const publish = (id: string, acceptLockout = false) =>
    commitPublish({
      variables: { input: { policyBundleQid: id, acceptLockout } },
      onCompleted: ({ publishPolicyBundle: res }) => {
        setPublishError(null);
        if (res.wouldLockYouOut) {
          setLockout({ id, why: res.wouldLockYouOut });
          return;
        }
        setLockout(null);
        onChanged();
      },
      onError: (e) => setPublishError(e.message),
    });

  const active = data.activePolicyBundle;
  return (
    <Stack gap="lg">
      <Section>
        <Group gap="sm" wrap="nowrap" align="flex-start">
          <StatusBadge tone={active ? "success" : "neutral"}>
            {active ? `Version ${active.version}` : "Built-in defaults"}
          </StatusBadge>
          <Text size="sm">
            {active ? (
              <>
                Published {active.publishedAt ? formatTime(active.publishedAt) : ""}
                {active.publishedBy ? ` by ${active.publishedBy.label}` : ""}. PDPs pick up a newly published bundle
                within about 10 seconds.
              </>
            ) : (
              "No bundle has been published. PDPs enforce the built-in default policies."
            )}
          </Text>
        </Group>
      </Section>

      <Section title="Bundles" description="Drafts, what's active, and what it replaced." flush>
        {publishError && (
          <Alert color="red" title="Publish failed" m="md">
            {publishError}
          </Alert>
        )}
        {lockout && (
          <Alert color="orange" title="This would lock you out" m="md">
            <Stack gap="xs">
              <Text size="sm">{lockout.why}</Text>
              <Group gap="xs">
                <Button size="xs" color="red" loading={isPublishing} onClick={() => publish(lockout.id, true)}>
                  Publish anyway
                </Button>
                <Button size="xs" variant="default" onClick={() => setLockout(null)}>
                  Cancel
                </Button>
              </Group>
            </Stack>
          </Alert>
        )}
        <Table.ScrollContainer minWidth={720}>
          <Table style={{ whiteSpace: "nowrap" }}>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>Version</Table.Th>
                <Table.Th>Status</Table.Th>
                <Table.Th>Created</Table.Th>
                <Table.Th>Published</Table.Th>
                <Table.Th />
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {data.policyBundles.map((b) => {
                const status = STATUS[b.status] ?? { label: b.status.toLowerCase(), tone: "neutral" as Tone };
                return (
                  <Table.Tr key={b.id}>
                    <Table.Td fw={600}>v{b.version}</Table.Td>
                    <Table.Td>
                      <StatusBadge tone={status.tone}>{status.label}</StatusBadge>
                    </Table.Td>
                    <Table.Td>
                      {b.createdAt ? formatTime(b.createdAt) : ""}
                      <Text span size="xs" c="dimmed">
                        {" "}
                        by {b.createdBy?.label ?? "unknown"}
                      </Text>
                    </Table.Td>
                    <Table.Td>{b.publishedAt ? formatTime(b.publishedAt) : "—"}</Table.Td>
                    <Table.Td>
                      {b.status === "DRAFT" && (
                        <Group gap="xs" justify="flex-end" wrap="nowrap">
                          <Button
                            size="xs"
                            variant="default"
                            onClick={() => setDryRun({ id: b.id, version: b.version })}
                          >
                            Dry run
                          </Button>
                          {data.canPublish && (
                            <Button size="xs" loading={isPublishing} onClick={() => publish(b.id)}>
                              Publish
                            </Button>
                          )}
                        </Group>
                      )}
                    </Table.Td>
                  </Table.Tr>
                );
              })}
            </Table.Tbody>
          </Table>
        </Table.ScrollContainer>
      </Section>

      {data.canPublish ? (
        <Editor
          key={active?.id ?? "none"}
          initialSchema={active?.schema ?? ""}
          initialPolicies={active?.policies ?? ""}
          onSaved={onChanged}
        />
      ) : (
        <Text size="sm" c="dimmed">
          You can read policy bundles but not change them.
        </Text>
      )}

      <Modal
        opened={dryRun !== null}
        onClose={() => setDryRun(null)}
        size="xl"
        title={<Text fw={600}>{dryRun ? `What draft version ${dryRun.version} would change` : ""}</Text>}
      >
        {dryRun && <DryRunReport draftQid={dryRun.id} />}
      </Modal>
    </Stack>
  );
}

/** Edits a new draft, starting from the active bundle. Saving validates it against the schema. */
function Editor({
  initialSchema,
  initialPolicies,
  onSaved,
}: {
  initialSchema: string;
  initialPolicies: string;
  onSaved: () => void;
}) {
  const [schema, setSchema] = useState(initialSchema);
  const [policies, setPolicies] = useState(initialPolicies);
  const [result, setResult] = useState<
    { kind: "saved"; version: number } | { kind: "invalid"; message: string } | null
  >(null);
  const [commitDraft, isSaving] = useMutation<PoliciesPageCreateDraftMutation>(graphql`
    mutation PoliciesPageCreateDraftMutation($input: CreatePolicyDraftInput!) {
      createPolicyDraft(input: $input) {
        bundle {
          id
          version
        }
        validationError
      }
    }
  `);

  const save = () =>
    commitDraft({
      variables: { input: { schema, policies } },
      onCompleted: ({ createPolicyDraft: res }) => {
        if (res.validationError) {
          setResult({ kind: "invalid", message: res.validationError });
        } else if (res.bundle) {
          setResult({ kind: "saved", version: res.bundle.version });
          onSaved();
        }
      },
      onError: (e) => setResult({ kind: "invalid", message: e.message }),
    });

  return (
    <Section
      title="New draft"
      description="Starts from the active bundle. Saving validates it against the schema; publishing is a separate step."
    >
      <Stack gap="sm">
        <Textarea
          id="policy-editor-policies"
          label="Policies (Cedar)"
          autosize
          minRows={8}
          styles={{ input: { fontFamily: "var(--mantine-font-family-monospace)" } }}
          value={policies}
          onChange={(e) => setPolicies(e.currentTarget.value)}
        />
        <Textarea
          id="policy-editor-schema"
          label="Schema (Cedar)"
          autosize
          minRows={6}
          styles={{ input: { fontFamily: "var(--mantine-font-family-monospace)" } }}
          value={schema}
          onChange={(e) => setSchema(e.currentTarget.value)}
        />
        {result?.kind === "invalid" && (
          <Alert color="red" title="Not saved: the bundle doesn't validate">
            <Text size="sm" ff="monospace" style={{ whiteSpace: "pre-wrap" }}>
              {result.message}
            </Text>
          </Alert>
        )}
        {result?.kind === "saved" && (
          <Alert color="awzy" title={`Saved draft version ${result.version}`}>
            Publish it from Bundles above to start enforcing it.
          </Alert>
        )}
        <Group>
          <Button onClick={save} loading={isSaving}>
            Save draft
          </Button>
        </Group>
      </Stack>
    </Section>
  );
}
