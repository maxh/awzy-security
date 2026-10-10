import { AppShell, Avatar, Badge, Box, Burger, Group, Menu, NavLink, Stack, Text, UnstyledButton } from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import { IconChevronDown, IconLogout } from "@tabler/icons-react";
import type { ReactElement, ReactNode } from "react";

import { AwzyWordmark } from "./KangarooLogo";

const SIDEBAR = "var(--mantine-color-ink-9)";

/** Sidebar and page frame shared by the client and corp portals. */
export function PortalShell({
  product,
  audience,
  nav,
  account,
  children,
}: {
  /** "Corp portal", "Client portal": under the wordmark. */
  product: string;
  audience: "client" | "corp";
  /** The sidebar's links: `ShellNavLink`s. */
  nav: ReactNode;
  /** Who is signed in, at the foot of the sidebar: a `ShellAccount`. */
  account?: ReactNode;
  children: ReactNode;
}) {
  const [opened, { toggle }] = useDisclosure();
  return (
    <AppShell
      header={{ height: { base: 52, sm: 0 } }}
      navbar={{ width: 236, breakpoint: "sm", collapsed: { mobile: !opened } }}
      padding={0}
    >
      {/* Only on small screens, where the sidebar folds away. */}
      <AppShell.Header hiddenFrom="sm" style={{ background: SIDEBAR, border: 0 }}>
        <Group h="100%" px="md">
          <Burger opened={opened} onClick={toggle} size="sm" color="white" aria-label="Menu" />
          <AwzyWordmark size={24} />
        </Group>
      </AppShell.Header>
      <AppShell.Navbar style={{ background: SIDEBAR, border: 0 }} p="md">
        <AppShell.Section visibleFrom="sm" mb="lg">
          <Group justify="space-between" align="center" wrap="nowrap">
            <AwzyWordmark product={product} />
            {audience === "corp" && (
              <Badge size="xs" variant="filled" color="grape">
                Staff
              </Badge>
            )}
          </Group>
        </AppShell.Section>
        <AppShell.Section grow>
          <Stack gap={2}>{nav}</Stack>
        </AppShell.Section>
        {account && <AppShell.Section mt="md">{account}</AppShell.Section>}
      </AppShell.Navbar>
      <AppShell.Main bg="var(--awzy-page)" mih="100vh">
        <Box maw={1280} mx="auto" px={{ base: "md", sm: 32 }} pt={28} pb={48}>
          {children}
        </Box>
      </AppShell.Main>
    </AppShell>
  );
}

/**
 * One sidebar link. `renderRoot` renders the router's link, so the portals keep their own routing:
 * `renderRoot={(props) => <Link to="/installs" {...props} />}`.
 */
export function ShellNavLink({
  label,
  icon,
  active,
  renderRoot,
}: {
  label: string;
  icon?: ReactNode;
  active?: boolean;
  renderRoot: (props: Record<string, unknown>) => ReactElement;
}) {
  return (
    <NavLink
      label={label}
      leftSection={icon}
      active={active}
      renderRoot={renderRoot}
      variant="filled"
      styles={{
        root: {
          borderRadius: "var(--mantine-radius-md)",
          color: active ? "white" : "var(--mantine-color-ink-1)",
          background: active ? "rgba(255, 255, 255, 0.09)" : undefined,
          fontWeight: active ? 600 : 500,
        },
        section: { color: active ? "var(--mantine-color-awzy-4)" : "var(--mantine-color-ink-3)" },
      }}
    />
  );
}

function initials(name: string): string {
  const parts = name.trim().split(/\s+/);
  return ((parts[0]?.[0] ?? "") + (parts.length > 1 ? (parts[parts.length - 1]?.[0] ?? "") : "")).toUpperCase();
}

/** The signed-in person at the foot of the sidebar, with a menu to sign out. */
export function ShellAccount({ name, detail, onSignOut }: { name: string; detail?: string; onSignOut: () => void }) {
  return (
    <Menu position="top-start" width={220} withinPortal>
      <Menu.Target>
        <UnstyledButton
          aria-label="Account menu"
          w="100%"
          p={8}
          style={{ borderRadius: "var(--mantine-radius-md)", background: "rgba(255, 255, 255, 0.05)" }}
        >
          <Group gap="sm" wrap="nowrap">
            <Avatar size={32} radius="xl" color="awzy" variant="filled">
              {initials(name)}
            </Avatar>
            <div style={{ flex: 1, minWidth: 0 }}>
              <Text size="sm" fw={600} c="white" truncate>
                {name}
              </Text>
              {detail && (
                <Text size="xs" c="ink.2" truncate>
                  {detail}
                </Text>
              )}
            </div>
            <IconChevronDown size={16} color="var(--mantine-color-ink-2)" />
          </Group>
        </UnstyledButton>
      </Menu.Target>
      <Menu.Dropdown>
        <Menu.Item leftSection={<IconLogout size={16} />} onClick={onSignOut}>
          Sign out
        </Menu.Item>
      </Menu.Dropdown>
    </Menu>
  );
}
