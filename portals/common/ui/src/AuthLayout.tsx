import { Box, Paper, Stack, Text, Title } from "@mantine/core";
import type { ReactNode } from "react";

import { AwzyWordmark } from "./KangarooLogo";

/** The sign-in page: awzy's panel on the left, the form on the right (stacked on small screens). */
export function AuthLayout({
  product,
  title,
  description,
  children,
}: {
  product: string;
  title: string;
  description?: ReactNode;
  children: ReactNode;
}) {
  return (
    <Box
      mih="100vh"
      style={{ display: "grid", gridTemplateColumns: "minmax(0, 1fr) minmax(0, 1fr)" }}
      className="awzy-auth"
    >
      <style>{`@media (max-width: 48em) { .awzy-auth { grid-template-columns: 1fr !important; } .awzy-auth-brand { display: none !important; } }`}</style>
      <Box
        className="awzy-auth-brand"
        p={48}
        style={{
          background:
            "radial-gradient(1200px 600px at 0% 100%, rgba(41, 169, 116, 0.22), transparent 60%), var(--mantine-color-ink-9)",
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
        }}
      >
        <AwzyWordmark product={product} size={32} />
        <Stack gap="sm" maw={440}>
          <Title order={1} c="white" fz={34} lh={1.15} style={{ letterSpacing: "-0.02em" }}>
            Every action, decided.
          </Title>
          <Text c="ink.2" size="md">
            Per-action, per-resource authorization for people and AI agents, running in your own cloud.
          </Text>
        </Stack>
        <Text c="ink.3" size="xs">
          awzy security
        </Text>
      </Box>
      <Box style={{ display: "flex", alignItems: "center", justifyContent: "center" }} p="xl">
        <Paper w="100%" maw={380} p={0} bg="transparent">
          <Stack gap={4} mb="lg">
            <Title order={2}>{title}</Title>
            {description && (
              <Text c="dimmed" size="sm">
                {description}
              </Text>
            )}
          </Stack>
          {children}
        </Paper>
      </Box>
    </Box>
  );
}
