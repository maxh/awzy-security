import { AppShell, Badge, Group, Title } from "@mantine/core";
import type { ReactNode } from "react";

import { KangarooLogo } from "./KangarooLogo";

/** Header and page frame shared by the client and corp portals. */
export function PortalShell({
  title,
  audience,
  nav,
  account,
  children,
}: {
  title: string;
  audience: "client" | "corp";
  /** Links shown in the header, after the title. */
  nav?: ReactNode;
  /** Who is signed in, shown at the right of the header. */
  account?: ReactNode;
  children: ReactNode;
}) {
  return (
    <AppShell header={{ height: 56 }} padding="md">
      <AppShell.Header>
        <Group h="100%" px="md" gap="sm">
          <KangarooLogo />
          <Title order={4}>{title}</Title>
          <Badge variant="light" color={audience === "corp" ? "grape" : "teal"}>
            {audience === "corp" ? "awzy staff" : "client"}
          </Badge>
          {nav && (
            <Group ml="xl" gap="lg">
              {nav}
            </Group>
          )}
          {account && (
            <Group ml="auto" gap="sm">
              {account}
            </Group>
          )}
        </Group>
      </AppShell.Header>
      <AppShell.Main>{children}</AppShell.Main>
    </AppShell>
  );
}
