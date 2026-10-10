# docs/site

The public documentation at https://docs.awzysecurity.com, built with
[Starlight](https://starlight.astro.build). Pages are Markdown in `src/content/docs/`; the install
guide is the repository's `INSTALL.md`, copied in by `scripts/sync.mjs` on every build.

```sh
cd docs/site
pnpm install --ignore-workspace
pnpm --ignore-workspace dev        # http://localhost:4321
```

It has its own lockfile, outside the portals' pnpm workspace and Bazel. CI builds it on pull
requests and deploys main to S3 behind CloudFront (`.github/workflows/docs-site.yml`,
`deploy/terraform/corp/corp-portal/docs-site.tf`).
