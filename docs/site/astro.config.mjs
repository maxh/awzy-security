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
        { label: "Start here", items: ["introduction", "concepts/how-awzy-decides"] },
        {
          label: "Concepts",
          items: [
            "concepts/principals-and-delegation",
            "concepts/monitor-and-enforce",
            "concepts/challenges-and-containment",
            "concepts/risk",
          ],
        },
        { label: "Guides", items: ["guides/install", "guides/writing-policies"] },
        { label: "Reference", items: ["reference/decision-api", "reference/security-model"] },
      ],
    }),
  ],
});
