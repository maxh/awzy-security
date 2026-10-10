# Investigation evals

Reports from `bazel run //authz/demo/evals:awzy-evals` against a real model, one file per run,
named `<date>-<model>.md` ([ADR 0067](../adr/0067-investigations-are-checked-findings-measured-against-a-compromised-model.md)).
Each says how many of the scenarios' attacks the model caught, how many ordinary days it left
alone, and how many injected instructions it followed. The checks' own gate, against a model that
follows every injection, runs in CI and isn't recorded here.
