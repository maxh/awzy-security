# 0106. Each cloud's demo has its own hostname

- Status: Accepted
- Date: 2026-10-11
- Amends: [0027](0027-demo-and-corp-hostnames.md) (the demo hostname)

## Context

0027 put the example client's install at `demo.awzysecurity.com`. That install runs on AWS, and
an example install on Google Cloud is being built. Two demos need two names, and each should say
which cloud it shows, since running the same product on every cloud is part of what it
demonstrates.

## Decision

- **`<cloud>.demo.awzysecurity.com`:** `aws.demo.awzysecurity.com` for the AWS example install,
  `gcp.demo.awzysecurity.com` for the Google Cloud one, and the same pattern for any other cloud.
  Each is a DNS-only Cloudflare CNAME to that install's load balancer, as in 0027.
- **`demo.awzysecurity.com` redirects to `aws.demo.awzysecurity.com`,** so links to the old name
  still work. The install serves the redirect itself: bootstrap's `ingress.redirect_hostnames`
  gives each older name an HTTPS listener with its own certificate and a route that redirects to
  the current hostname, path and all. It's a bootstrap setting any install can use when its
  hostname changes, so it needs nothing of Cloudflare beyond DNS.

## Consequences

- The dev IdP's issuer follows the hostname (0032), so it's `https://aws.demo.awzysecurity.com/idp`;
  sessions from before the move sign in again.
- `demo.awzysecurity.com` could later become a page choosing between the demos, instead of a
  redirect.
- `corp.awzysecurity.com` (0027) is unchanged.
