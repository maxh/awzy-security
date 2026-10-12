// https://docs.awzysecurity.com: awzy's public documentation, built with Starlight.
import starlight from "@astrojs/starlight";
import { defineConfig, passthroughImageService } from "astro/config";

export default defineConfig({
  site: "https://docs.awzysecurity.com",
  // No image processing, so no native sharp dependency in CI.
  image: { service: passthroughImageService() },
  integrations: [
    starlight({
      title: "awzy",
      description: "Per-action authorization for people and AI agents, in your own cloud.",
      logo: { src: "./src/assets/kangaroo.svg" },
      favicon: "/favicon.svg",
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/maxh/awzy-security" }],
      editLink: { baseUrl: "https://github.com/maxh/awzy-security/edit/main/docs/site/" },
      customCss: ["./src/styles/theme.css"],
      sidebar: [
        { label: "Start here", items: ["introduction", "principles", "concepts/how-awzy-decides", "client-portal"] },
        {
          label: "Workflows",
          items: [
            "workflows/security-team",
            "workflows/employees",
            "workflows/approvers",
            "workflows/agent-builders",
            "workflows/platform-team",
          ],
        },
        {
          label: "Concepts",
          items: [
            "concepts/principals-and-delegation",
            "concepts/monitor-and-enforce",
            "concepts/challenges-and-containment",
            "concepts/risk",
            "concepts/investigations",
          ],
        },
        {
          label: "Guides",
          items: [
            "guides/install",
            "guides/connecting-your-idp",
            "guides/upgrading",
            "guides/monitoring",
            "guides/troubleshooting",
            "guides/writing-policies",
            "guides/mcp-gateway",
            "guides/typescript-sdk",
            "guides/sending-events",
            "guides/connectors",
            "guides/optional-features",
            "guides/break-glass",
            "guides/data-retention",
          ],
        },
        {
          label: "Reference",
          items: [
            "reference/configuration",
            "reference/services",
            "reference/decision-api",
            "reference/domain-model",
            "reference/security-model",
            "reference/glossary",
          ],
        },
      ],
    }),
  ],
});
