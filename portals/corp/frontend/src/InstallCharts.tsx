import { LineChart, type LinePoint, PageLoader, Section, StatusBadge } from "@awzy/ui";
import { Card, Group, SegmentedControl, SimpleGrid, Stack, Text } from "@mantine/core";
import { type ReactNode, Suspense, useState, useTransition } from "react";
import { graphql, useLazyLoadQuery } from "react-relay";

import type { InstallChartsQuery } from "./__generated__/InstallChartsQuery.graphql";

const WINDOWS = {
  DAY: { label: "24 hours", ms: 86_400_000, bucketMs: 300_000 },
  WEEK: { label: "7 days", ms: 7 * 86_400_000, bucketMs: 1_800_000 },
  MONTH: { label: "30 days", ms: 30 * 86_400_000, bucketMs: 7_200_000 },
} as const;
type Span = keyof typeof WINDOWS;
const SPANS: readonly Span[] = ["DAY", "WEEK", "MONTH"];

/** The install's heartbeats over time: one chart per figure, worst value per bucket. */
export function InstallCharts({ id }: { id: string }) {
  const [span, setSpan] = useState<Span>("DAY");
  const [pending, startTransition] = useTransition();
  return (
    <Section
      title="Over time"
      description="Gaps are time the install wasn't reporting."
      actions={
        <SegmentedControl
          size="xs"
          value={span}
          onChange={(v) => startTransition(() => setSpan(v))}
          data={SPANS.map((value) => ({ value, label: WINDOWS[value].label }))}
        />
      }
    >
      <Suspense fallback={<PageLoader />}>
        <div style={{ opacity: pending ? 0.5 : 1, transition: "opacity 150ms" }}>
          <Charts key={span} id={id} span={span} />
        </div>
      </Suspense>
    </Section>
  );
}

function Charts({ id, span }: { id: string; span: Span }) {
  const data = useLazyLoadQuery<InstallChartsQuery>(
    graphql`
      query InstallChartsQuery($id: ID!, $window: MetricWindow!) {
        node(id: $id) {
          ... on Install {
            metricHistory(window: $window) {
              at
              decisionsPerSecond
              pdpP99Ms
              errorRate
              kafkaLag
              clickhouseDiskUsedPercent
              containmentFeedAgeSeconds
              policySyncAgeSeconds
              principalsSyncAgeSeconds
              connectorSyncAgeSeconds
              gatewayPdpUnreachable
              degraded
            }
          }
        }
      }
    `,
    { id, window: span },
    { fetchPolicy: "store-and-network" },
  );
  const w = WINDOWS[span];
  const samples = data.node?.metricHistory ?? [];
  // The window ends when it was chosen (the component remounts per window), not each render.
  const [to] = useState(() => Date.now());
  const from = to - w.ms;
  const series = (pick: (s: (typeof samples)[number]) => number | null | undefined): LinePoint[] =>
    samples.map((s) => ({ at: new Date(s.at).getTime(), value: pick(s) ?? null }));
  const formatTime = (at: number) =>
    new Date(at).toLocaleString([], {
      weekday: "short",
      ...(span === "DAY" ? {} : { month: "short", day: "numeric" }),
      hour: "2-digit",
      minute: "2-digit",
    });
  // The axis counts back from now, so its two ends never read as the same clock time.
  const formatAxisTime = (at: number) => {
    const hours = Math.round((to - at) / 3_600_000);
    if (hours === 0) return "now";
    return hours < 48 ? `${hours}h ago` : `${Math.round(hours / 24)}d ago`;
  };
  const common = { from, to, maxGap: w.bucketMs * 1.5, formatTime, formatAxisTime };
  const degraded = samples.filter((s) => s.degraded).length;
  // Freshness (ADR 0068): each card only when the install reports it, since not every install
  // runs awzy-challenge, awzy-context, a connector, or awzy-gateway.
  const freshness = [
    { title: "Containment feed age", note: "worst PDP; target 2 s", pick: (s: Sample) => s.containmentFeedAgeSeconds },
    { title: "Policy sync age", note: "worst PDP; target 60 s", pick: (s: Sample) => s.policySyncAgeSeconds },
    { title: "Principals sync age", note: "worst PDP; target 60 s", pick: (s: Sample) => s.principalsSyncAgeSeconds },
    { title: "Connector sync age", note: "stalest connector", pick: (s: Sample) => s.connectorSyncAgeSeconds },
  ].filter((f) => samples.some((s) => f.pick(s) != null));
  const outageCalls = samples.some((s) => s.gatewayPdpUnreachable != null);

  if (samples.length === 0) {
    return (
      <Text c="dimmed" size="sm">
        No heartbeats in the last {w.label}.
      </Text>
    );
  }
  return (
    <Stack gap="sm">
      <Group gap="xs">
        <StatusBadge tone={degraded ? "warning" : "success"}>
          {degraded ? "Degraded at times" : "No degraded periods"}
        </StatusBadge>
        <Text size="sm" c="dimmed">
          {degraded
            ? `${degraded} of ${samples.length} intervals had a service that wasn't fully ready.`
            : `Every service was ready in all ${samples.length} intervals with a heartbeat.`}
        </Text>
      </Group>
      <SimpleGrid cols={{ base: 1, md: 2 }}>
        <ChartCard title="PDP p99" note="worst per interval">
          <LineChart
            label="PDP p99 latency"
            points={series((s) => s.pdpP99Ms)}
            format={(v) => (v === 0 ? "0 ms" : `${v.toFixed(v < 10 ? 1 : 0)} ms`)}
            {...common}
          />
        </ChartCard>
        <ChartCard title="Error rate" note="worst per interval">
          <LineChart label="Error rate" points={series((s) => s.errorRate)} format={percentOfOne} {...common} />
        </ChartCard>
        <ChartCard title="Decisions per second" note="average per interval">
          <LineChart
            label="Decisions per second"
            points={series((s) => s.decisionsPerSecond)}
            format={(v) => v.toLocaleString(undefined, { maximumFractionDigits: 1 })}
            {...common}
          />
        </ChartCard>
        <ChartCard title="Kafka lag" note="worst per interval">
          <LineChart
            label="Kafka consumer lag"
            points={series((s) => s.kafkaLag)}
            format={(v) => Math.round(v).toLocaleString()}
            {...common}
          />
        </ChartCard>
        <ChartCard title="ClickHouse disk used" note="worst per interval">
          <LineChart
            label="ClickHouse disk used"
            points={series((s) => s.clickhouseDiskUsedPercent)}
            format={(v) => `${Math.round(v)}%`}
            max={100}
            {...common}
          />
        </ChartCard>
        {freshness.map((f) => (
          <ChartCard key={f.title} title={f.title} note={f.note}>
            <LineChart label={f.title} points={series(f.pick)} format={seconds} {...common} />
          </ChartCard>
        ))}
        {outageCalls && (
          <ChartCard title="Gateway calls without the PDP" note="total per interval">
            <LineChart
              label="Tool calls awzy-gateway made while it couldn't reach awzy-pdp"
              points={series((s) => s.gatewayPdpUnreachable)}
              format={(v) => Math.round(v).toLocaleString()}
              {...common}
            />
          </ChartCard>
        )}
      </SimpleGrid>
    </Stack>
  );
}

type Sample = NonNullable<NonNullable<InstallChartsQuery["response"]["node"]>["metricHistory"]>[number];

/** An age in seconds, in the largest unit that keeps it readable. */
function seconds(v: number): string {
  if (v < 60) return `${v.toFixed(v < 10 ? 1 : 0)} s`;
  if (v < 3600) return `${Math.round(v / 60)} min`;
  return `${(v / 3600).toFixed(1)} h`;
}

function ChartCard({ title, note, children }: { title: string; note: string; children: ReactNode }) {
  return (
    <Card padding="sm" shadow="none">
      <Group gap={6} mb={4}>
        <Text size="sm" fw={600}>
          {title}
        </Text>
        <Text size="xs" c="dimmed">
          {note}
        </Text>
      </Group>
      {children}
    </Card>
  );
}

/** 0 to 1 as a percentage: "0%", "0.12%", "1.5%". */
function percentOfOne(v: number): string {
  if (v === 0) return "0%";
  const pct = v * 100;
  return `${pct.toFixed(pct < 1 ? 2 : 1)}%`;
}
