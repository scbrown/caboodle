# Using caboodle

Caboodle ships a resumable interview and the plan/apply/verify engine with
adapters for Quipu, Camayoc, Bobbin, Yupana, and Desire Path.

```bash
cargo install --git https://github.com/scbrown/caboodle --locked

# Guided: use, crew themes, and anticipated graph questions are checkpointed.
caboodle init --guided

# Non-interactive: read the same schema from caboodle-intent.toml.
caboodle plan --profile retrieval --intent caboodle-intent.toml

# Converge the reviewed plan, then prove both tools with isolated round trips.
caboodle apply
caboodle verify
caboodle verify-questions

# Or run both phases together after review.
caboodle install
```

`apply` installs a missing released tool and reads its version back — a
successful installer exit alone is never accepted. `verify` uses temporary
isolated stores: it proves a marker absent, writes it, and requires the reader
path to return it. Progress is written atomically to `.caboodle/state.json`, so
rerunning converges.

If input ends partway through an interview, Caboodle stops without a plan and
keeps accepted answers in `.caboodle/interview.toml`. Run `caboodle init
--guided` again to resume at the next unanswered question. Invalid answers are
rejected and never produce a plan.

Use `--skip-install` when package installation belongs to another system; the
version and functional checks still run.

`code-intel` adds checksum-pinned Yupana and proves a caller edge in a temporary
repository with isolated HOME/state. `everything` additionally installs Desire
Path from its pinned public revision (upstream has no tag/release yet), reads
that revision back from `dp version`, and proves record/list against a temporary
database. These checks never use the user's normal evidence stores.

`caboodle-intent.toml` names the intended use, zero or more themed crew members,
and at least one anticipated ontology question. A question is a contract, not a
wish: it includes an answer shape, fixture/seed intent, executable `SELECT` or
`ASK` SPARQL, and an expected result marker. The reviewed install plan embeds
the contract, and `verify-questions` executes it through Quipu's reader path.
This makes “what should the graph answer?” an install acceptance test.

```toml
intended_use = "answer dependency and ownership questions about my services"

[[crew_members]]
name = "Harbor"
theme = "patient navigator"
domain = "service operations"
role = "trace dependencies and explain incidents"

[[anticipated_questions]]
question = "which services depend on the message broker?"
answer_shape = "a list of service entities"
seed_intent = "two fixture services and one depends_on edge to the fixture broker"
sparql = "SELECT ?service WHERE { ?service <https://example.org/depends_on> <https://example.org/broker> }"
expected = "fixture-service"
```

The prose fields steer ontology and seed construction; the SPARQL and expected
marker decide whether that construction is retrievable through the path a reader
will actually use. Never put credentials in this file or the generated plan.
