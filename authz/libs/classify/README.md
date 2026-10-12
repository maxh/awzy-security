# classify · sensitivity classification

Places a resource in a sensitivity tier (`public`, `internal`, `confidential`, `crown_jewels`)
from what a connector sends, without a model
([ADR 0047](../../../docs/adr/0047-metadata-first-classification-rules.md)). Part of
[awzy-authz](../../).

A sensitivity label decides; marked content (a text excerpt saying "Confidential", "Approved
for external distribution") decides next; names and folder paths ("Board", "Payroll",
"Contract") decide only when the others are silent, and never make anything public. A label is never lowered: content marked higher
raises it. What none of these places stays unclassified, for the model step (not built yet).

awzy-context runs it in the background as resources are written. awzy-synthetic's tests score it
against the synthetic company's ground truth.
