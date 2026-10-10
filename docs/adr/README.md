# Architecture decision records

One file per decision, numbered in the order it was made. Each record has a status, the context
behind the decision, the decision itself, and its consequences. Records are not edited after they
are accepted. To change a decision, add a new record and mark the old one `Superseded by NNNN`.

The [platform plan](../platform-plan.md) describes the system as
a whole. These records keep track of why it looks that way.

There's no index to keep in sync: the file names are the index (`ls docs/adr`), and each record
starts with its title, status, and date. To find what's decided about a topic,
`grep -ril '<topic>' docs/adr`.

## Not decided yet

These are tracked in the plan's open questions and get a record once they are settled:

- Who operates ClickHouse in a client install: the client's team with runbooks, or ClickHouse Cloud BYOC.
- How the GCP example client gets its own organization: a second domain or a subdomain (see 0018).
- Whether the `small` profile drops Kafka. (Kafka stays in the default profile; see 0005.)
- How often awzy-gateway has to issue agent tokens itself because the client IdP can't.
- How awzy's services authenticate to each other (mTLS, a service mesh, or cloud workload
  identity). Until then awzy-policy's API is open to anything that reaches it in the cluster, and
  a network policy is the only guard (see 0020).
