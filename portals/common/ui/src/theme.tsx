import "@fontsource-variable/inter/index.css";

import { createTheme, type CSSVariablesResolver, MantineProvider, type MantineColorsTuple } from "@mantine/core";
import type { ReactNode } from "react";

/** The kangaroo's green, from light to dark: buttons, links, and the active nav item. */
const awzy: MantineColorsTuple = [
  "#e8f8f1",
  "#d5eee3",
  "#a9dcc6",
  "#7ac9a7",
  "#53b98d",
  "#39af7d",
  "#29a974",
  "#199363",
  "#0c8357",
  "#007148",
];

/** Navy for the sidebar and dark surfaces; 9 is the sidebar itself. */
const ink: MantineColorsTuple = [
  "#eef1f7",
  "#dce1ec",
  "#b5c0d7",
  "#8b9dc2",
  "#6880b0",
  "#526ea6",
  "#4664a2",
  "#37538e",
  "#2d4980",
  "#0d1526",
];

/** Shared Mantine theme for both portals. Clients may later override `primaryColor`. */
export const awzyTheme = createTheme({
  primaryColor: "awzy",
  primaryShade: { light: 7, dark: 5 },
  colors: { awzy, ink },
  fontFamily: "Inter Variable, -apple-system, Segoe UI, Roboto, sans-serif",
  fontFamilyMonospace: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
  headings: { fontWeight: "650", sizes: { h1: { fontSize: "1.625rem" }, h2: { fontSize: "1.375rem" } } },
  defaultRadius: "md",
  cursorType: "pointer",
  components: {
    Badge: { defaultProps: { radius: "sm", tt: "none", fw: 600 } },
    Card: { defaultProps: { withBorder: true, radius: "md", padding: "lg" } },
    Paper: { defaultProps: { radius: "md" } },
    Modal: { defaultProps: { radius: "md", centered: true } },
    Table: {
      defaultProps: { verticalSpacing: "sm", horizontalSpacing: "md", highlightOnHover: true },
      styles: {
        th: {
          fontSize: "var(--mantine-font-size-xs)",
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: "0.04em",
          color: "var(--mantine-color-dimmed)",
        },
      },
    },
  },
});

/** `--awzy-page`: behind the cards, a step away from them in both color schemes. */
const resolver: CSSVariablesResolver = () => ({
  variables: {},
  light: { "--awzy-page": "#f6f7f9" },
  dark: { "--awzy-page": "var(--mantine-color-dark-8)" },
});

export function AwzyProvider({ children }: { children: ReactNode }) {
  return (
    <MantineProvider theme={awzyTheme} defaultColorScheme="auto" cssVariablesResolver={resolver}>
      {children}
    </MantineProvider>
  );
}
