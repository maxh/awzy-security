import { createTheme, MantineProvider } from "@mantine/core";
import type { ReactNode } from "react";

/** Shared Mantine theme for both portals. Clients may later override `primaryColor`. */
export const awzyTheme = createTheme({
  primaryColor: "teal",
  fontFamily: "IBM Plex Sans, -apple-system, Segoe UI, Roboto, sans-serif",
  fontFamilyMonospace: "JetBrains Mono, ui-monospace, Menlo, monospace",
  defaultRadius: "sm",
});

export function AwzyProvider({ children }: { children: ReactNode }) {
  return (
    <MantineProvider theme={awzyTheme} defaultColorScheme="auto">
      {children}
    </MantineProvider>
  );
}
