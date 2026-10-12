import { Anchor, Badge, Button, Card, Center, Group, Loader, Stack, Text, ThemeIcon, Title } from "@mantine/core";
import { IconArrowLeft } from "@tabler/icons-react";
import type { ReactElement, ReactNode } from "react";

/**
 * The top of a page: what it is, a sentence on what it's for, and its actions. `back` links to the
 * page above it, rendered by the router: `back={{ label: "Installs", renderRoot: (p) => <Link
 * to="/installs" {...p} /> }}`.
 */
export function PageHeader({
  title,
  description,
  badge,
  actions,
  back,
}: {
  title: ReactNode;
  description?: ReactNode;
  badge?: ReactNode;
  actions?: ReactNode;
  back?: { label: string; renderRoot: (props: Record<string, unknown>) => ReactElement };
}) {
  return (
    <Stack gap={6} mb="xl">
      {back && (
        <Anchor size="sm" c="dimmed" renderRoot={back.renderRoot} underline="hover">
          <Group gap={4}>
            <IconArrowLeft size={14} />
            {back.label}
          </Group>
        </Anchor>
      )}
      <Group justify="space-between" align="flex-start" gap="md">
        <Stack gap={4} style={{ flex: "1 1 320px" }}>
          <Group gap="sm">
            <Title order={2}>{title}</Title>
            {badge}
          </Group>
          {description && (
            <Text c="dimmed" size="sm" maw={720}>
              {description}
            </Text>
          )}
        </Stack>
        {actions && (
          <Group gap="sm" wrap="nowrap">
            {actions}
          </Group>
        )}
      </Group>
    </Stack>
  );
}

/** A titled panel on a page: a table, a chart, a form. */
export function Section({
  title,
  description,
  actions,
  children,
  flush,
}: {
  title?: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  /** No padding inside, for a table that runs to the edges. */
  flush?: boolean;
}) {
  return (
    <Card padding={0}>
      {(title || actions) && (
        <Group
          justify="space-between"
          px="lg"
          py="md"
          style={{ borderBottom: "1px solid var(--mantine-color-default-border)" }}
        >
          <div>
            {title && (
              <Text fw={600} size="md">
                {title}
              </Text>
            )}
            {description && (
              <Text size="sm" c="dimmed">
                {description}
              </Text>
            )}
          </div>
          {actions}
        </Group>
      )}
      <div style={{ padding: flush ? 0 : "var(--mantine-spacing-lg)" }}>{children}</div>
    </Card>
  );
}

export type Tone = "success" | "warning" | "danger" | "neutral" | "info";

const TONE_COLOR: Record<Tone, string> = {
  success: "awzy",
  warning: "yellow",
  danger: "red",
  neutral: "gray",
  info: "blue",
};

/** A state, with a colored dot: healthy, degraded, never reported. */
export function StatusBadge({ tone, children }: { tone: Tone; children: ReactNode }) {
  const color = TONE_COLOR[tone];
  return (
    <Badge
      color={color}
      variant="light"
      style={{ flexShrink: 0, minWidth: "max-content" }}
      leftSection={
        <span
          style={{
            display: "block",
            width: 6,
            height: 6,
            borderRadius: 6,
            background: `var(--mantine-color-${color}-filled)`,
          }}
        />
      }
    >
      {children}
    </Badge>
  );
}

/** What a page or table shows when there's nothing in it yet, and what to do about it. */
export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon?: ReactNode;
  title: string;
  description?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <Stack align="center" gap="xs" py={48} px="md" ta="center">
      {icon && (
        <ThemeIcon size={44} radius="xl" variant="light" color="gray" mb={4}>
          {icon}
        </ThemeIcon>
      )}
      <Text fw={600}>{title}</Text>
      {description && (
        <Text size="sm" c="dimmed" maw={420}>
          {description}
        </Text>
      )}
      {action && <div style={{ marginTop: 8 }}>{action}</div>}
    </Stack>
  );
}

/** A centered spinner, for Suspense fallbacks. */
export function PageLoader() {
  return (
    <Center p={48}>
      <Loader size="sm" />
    </Center>
  );
}

/** One figure with its label, for a row of key numbers. */
export function Stat({ label, value, hint }: { label: string; value: ReactNode; hint?: ReactNode }) {
  return (
    <Card padding="md">
      <Text size="xs" c="dimmed" fw={600} tt="uppercase" style={{ letterSpacing: "0.04em" }}>
        {label}
      </Text>
      <Text fw={650} fz={22} mt={4} style={{ fontVariantNumeric: "tabular-nums" }}>
        {value}
      </Text>
      {hint && (
        <Text size="xs" c="dimmed" mt={2}>
          {hint}
        </Text>
      )}
    </Card>
  );
}

/**
 * The foot of a list that starts short: loads the next page of its connection while there is one.
 * `onMore` is usually `() => loadNext(n)` from Relay's `usePaginationFragment`.
 */
export function ShowMore({ hasMore, loading, onMore }: { hasMore: boolean; loading: boolean; onMore: () => void }) {
  if (!hasMore) return null;
  return (
    <Center py="sm">
      <Button variant="subtle" size="xs" loading={loading} onClick={onMore}>
        Show more
      </Button>
    </Center>
  );
}
