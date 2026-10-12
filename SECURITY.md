# Security

awzy decides who and what may act on a company's data, so a flaw in it is a flaw in every
install's access control. We want to hear about anything you find.

## Reporting a vulnerability

Report it privately through GitHub: on this repository's **Security** tab, choose **Report a
vulnerability**. Please don't open a public issue or pull request for it.

Include what you can of:

- what's affected (a service, the chart, the Terraform, a portal), and the commit or release
- how to reproduce it, or a proof of concept
- what an attacker gains, and from what position (an IdP user, an agent with a token, a pod in the
  cluster, someone on the network)

We'll acknowledge your report within a few working days, keep you updated while we fix it, and
credit you in the fix's release notes unless you'd rather not be named. Please give us a
reasonable time to fix it before you disclose it.

## Scope

In scope: the code in this repository, the images and Helm chart built from it, and the
Terraform that installs it.

The hosted demo (`aws.demo.awzysecurity.com`) runs the same code with sample data. Findings there
are welcome, with two exceptions that are deliberate:

- **Anyone can sign in to the demo, with no password.** It runs the development IdP
  (`tools/dev-idp`, ADR 0032), whose signing key is fixed and derivable from this source, so
  minting a token it accepts is expected. A real install never runs the development IdP or
  trusts its issuer; a way to make one do so is in scope.
- **The demo's data is synthetic.** Larkspur Capital and its people don't exist.

Out of scope: denial of service against the demo, social engineering, and findings that need a
compromised cluster admin or cloud account, which by design can change anything in an install.

## Supported versions

awzy is pre-release. Fixes land on `main` and in the next release; there are no maintained older
release lines yet.

## Good faith

We won't pursue or support legal action against research done in good faith under this policy:
you avoid privacy violations and disruption, don't access data that isn't yours beyond what's
needed to show the issue, and report it to us first.

## How awzy is secured

[PRINCIPLES.md](PRINCIPLES.md) states the rules the design follows. The decision records in
[docs/adr/](docs/adr/) explain individual choices: the portal authorized by the PDP (0020), service
tokens between services (0055), signed policy bundles and modes (0080, 0092), break-glass (0022),
and support access that the client approves each time (0095), among others.
