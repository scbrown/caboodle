# txtai vs. the quipu stack — comparison and gap analysis

> **Status: analysis, 2026-08-31.** Compared [txtai](https://github.com/neuml/txtai)
> v9.14.0 (commit `546e777`, source examined directly) against the stack as it
> exists on disk today: quipu v0.3.27, bobbin v0.10.4, yupana v0.6.4, camayoc,
> shuttle v0.1.0 (source examined), plus the harness layer (shantytown, creel,
> skein). Every claim about
> our side was checked against source, not README ambition; txtai claims come
> from its source tree and docs. Gaps are proposals, not commitments — each
> should become a bead/pitch in its home repo before anyone implements it.

## One-line framing

**txtai** is a Python "all-in-one AI framework": an embeddings database (dense +
sparse vector indexes, relational content store, similarity graph) with ~30
model-powered pipelines, YAML workflows, an agent framework, and answer
generation built in. Batteries included; governance absent.

**The quipu stack** is the opposite decomposition: a bitemporal, governed RDF
store (quipu) with an admission-control layer (camayoc), a code-context
retrieval engine (bobbin), structural code intelligence (yupana), and agent
harnesses (shantytown/creel) — where *no component ever calls an LLM*, facts
are refusable/attributable/time-travelable, and generation belongs to the
agent, outside the stack.

They overlap on exactly one axis — embeddings-backed retrieval over a
knowledge/graph substrate — and diverge everywhere else on purpose.

## Comparison at a glance

| Axis | txtai 9.14 | quipu stack today |
|---|---|---|
| Language / license | Python 3.10+, Apache 2.0 | Rust (quipu/bobbin/yupana), Python scripts (camayoc), MIT |
| Core data model | Embeddings DB: dense+sparse ANN ∪ SQL content store ∪ similarity graph | Bitemporal EAVT fact log → RDF quads, SPARQL 1.1, SHACL/OWL gates (quipu); LanceDB chunk store (bobbin) |
| Dense ANN backends | Faiss, HNSW, Annoy, NumPy, Torch, pgvector, sqlite-vec, Milvus, ggml, more | Bobbin: LanceDB **flat scan, no ANN index**; quipu: SQLite brute-force cosine (LanceDB ANN exists but is excluded from release binaries) |
| Sparse / keyword | BM25/TF-IDF/SIF + learned sparse ANN (ivfsparse, pgsparse) | Tantivy BM25 FTS (bobbin); `FILTER(CONTAINS)` fallback (quipu release) |
| Hybrid fusion | Dense + sparse convex combination | RRF + recency/tag/repo-affinity/feedback boosts + personalized PageRank (bobbin); SPARQL∩vector `hybrid_search` (quipu) |
| Reranking | Cross-encoder `Reranker`/`Similarity` pipelines | **None neural** — deterministic boosts and PPR re-ordering only |
| SQL-with-`similar()` | Yes, over SQLite/DuckDB/any RDBMS | SPARQL with temporal params; stored/parameterized queries; no SQL surface |
| Graph | Auto-built similarity graph, topic modeling, openCypher, GraphRAG traversal | Declared-fact RDF graph; petgraph projection: PageRank/PPR/Louvain/impact/paths (quipu); chunk+coupling edges (bobbin); call graph (yupana) |
| Document ingestion | Textractor (PDF/Office/HTML), segmentation/chunking, tabular, URL retrieve | Code + markdown chunking, opt-in PDF *text* only (bobbin); **no generic document loader** (quipu embeds per-entity, no chunking) |
| Multimodal | Text, image (CLIP/caption/objects), audio (transcribe/TTS), video | PDF text extraction; image captioning named-and-deferred; audio/video: no seam |
| LLM / RAG generation | First-class: LLM pipeline (HF, llama.cpp, LiteLLM), RAG incl. GraphRAG, guided generation | **Deliberately none.** Stack retrieves and assembles; the agent generates |
| Pipelines / workflows | ~30 pipelines, YAML workflows, cron scheduling — an in-process *executor* of unsigned data pipelines | Shuttle: declared state machines, per-agent ed25519-**signed** transitions, JSONL hot path, export into quipu's time-windowed graphs, deep-freezable history — a governed *record*, deliberately no daemon/queue/executor; execution belongs to agents (shantytown/creel) and skills (skein) |
| Agents | Built-in agent framework with tools (search, bash, file edit, grep) | The stack *serves* agents (Claude Code, codex) rather than shipping one |
| API / bindings | FastAPI, MCP, OpenAI-compatible endpoints, cluster sharding; JS/Java/Rust/Go bindings | REST + MCP (44 quipu tools, 31 bobbin, 15 yupana), CLIs; **no Python bindings** (quipu 🔜) |
| Temporal | None (index-time snapshot) | Bitemporal everywhere — even embeddings carry `valid_from`/`valid_to`; time-travel queries |
| Governance / provenance | None (auth token on API) | SHACL/OWL write gates, trust-label lattice, quarantine planes, authority-gated promotion, signed ed25519 verdicts, `write.refused` event stream, certified packs |
| Observability | MLflow tracing integration | Prometheus `/metrics` with bounded cardinality (quipu, bobbin), JSONL spools (yupana), caboodle scrape/alert/dashboard generation |
| Scale-out | Index sharding via API cluster; cloud/object-storage index sync | Read-side federation with label composition (quipu); multi-repo + RBAC (bobbin); no replication (🔜) |

## Where the stack is ahead (no txtai counterpart)

Worth stating so the gap list below is read in proportion:

- **Everything governance**: refusal-at-write (SHACL/OWL/policy/authority),
  trust planes with quarantine and authority-gated promotion, bitemporal
  time-travel, signed verdicts and deterministic `T ⊨ Σ` audit, provenance-
  refusing ingress, certified knowledge packs with dual signatures, and
  signed workflow history (shuttle: every run transition ed25519-signed by
  the performing agent, re-verifiable from the exported graph alone, with
  completed windows deep-frozen and still queryable). txtai has
  no analogue for any of this; its "graph" is similarity clusters, not an
  ontology.
- **Structural code intelligence** (yupana): tier-tagged AST/call-graph facts,
  per-tenant copy-on-write overlays, pre-edit guards, blast radius. txtai has
  nothing code-aware at all.
- **Agent-context engineering** (bobbin): hook-driven context injection with
  gating, token budgets, doc→source bridging, cross-agent feedback loops, and
  a real eval harness with LLM-judge scoring. txtai retrieves; it does not
  decide *when and how much* to inject into a coding agent.
- **Honest serving**: coverage/label reporting on every answer, `NO COVERAGE`
  verdicts, freshness omitted-not-faked, negative controls on every proof.

## Gaps worth implementing

Ordered roughly by (value ÷ effort), each with a proposed home.

### G1. Second-stage neural reranking — bobbin

txtai ships cross-encoder reranking (`Reranker`, `Similarity` pipelines) as a
standard post-retrieval stage. Bobbin's ranking is RRF plus deterministic
multiplicative boosts plus PPR re-ordering — grep confirms no cross-encoder
anywhere. Bobbin already embeds via ONNX with a model registry and GPU
detection, so the runtime cost is config, not architecture: an opt-in
`[search] reranker` that scores the top-K (e.g. 50) hybrid results with a
small ONNX cross-encoder (`ms-marco-MiniLM` class) before context assembly.
The eval harness can prove or refute the gain — which is exactly the kind of
check the stack culture demands before shipping a ranking change.

### G2. ANN indexing at scale — bobbin and quipu release engineering

txtai treats ANN backends as a first-class pluggable axis (Faiss, HNSW,
pgvector, …). Today bobbin's vector search is a **flat scan** with an explicit
`SCAN_ALL_LIMIT` (only the FTS index is built), and quipu's default is
brute-force cosine in SQLite with the LanceDB ANN path **excluded from
release binaries** (a release asked for `vector.backend = "lancedb"` refuses
at startup). Both are honest and fine at current scale (<1M facts, repo-sized
corpora) — but the ceiling is documented, not solved. Proposal, in two
independent halves:

1. bobbin: create a LanceDB vector index (IVF_PQ or HNSW) once a table
   crosses a size threshold, benchmarked by `bobbin benchmark`;
2. caboodle: offer a `lancedb`-enabled quipu build in the install plan (or a
   pinned alternate release asset), so the choice is a reviewed plan line
   rather than a from-source adventure.

### G3. Generic document ingestion (a Textractor equivalent) — bobbin

txtai's data pipelines (Textractor, FileToHTML, HTMLToMarkdown, Segmentation,
Tabular, URLRetrieve) turn arbitrary documents into indexed chunks. Bobbin's
entire non-code surface today is: markdown (excellent), line-chunked config
formats, opt-in PDF *text* extraction (`MULTIMODAL_EXTENSIONS = ["pdf"]`),
plus bespoke `ChunkSource` impls for SQL rows, beads, and chat archives —
each hand-built, none user-definable. A `documents` source that handles
HTML→markdown and office formats through the existing hashed-source
incremental lifecycle would close most of the distance deterministically (no
OCR, no vision, no model in the loop — consistent with camayoc rule 3). The
`ChunkSource` trait already factored in `src/index/source.rs` is the seam.

### G4. Quarantined extraction pipelines — camayoc

txtai ships Entity/NER and zero-shot labeling pipelines that feed its graph.
The stack's equivalent is not "add NER to quipu" — camayoc's ingress rule 4
already defines the right shape: *inference is quarantined, not banned*. What
exists today is one tool (bobbin's `knowledge_inferred_extract`, trust-rank-0,
quarantine-enveloped). The gap is a small family of camayoc producer scripts
that extract entities/relations from prose deterministically first (regex,
gazetteers from the graph itself) and optionally by model, landing in
`crew:inferred` with promotion eligibility. This is the one gap where txtai's
feature maps *cleanly* onto an already-designed slot in our architecture.

### G5. Similarity edges and topics in the graph lane — bobbin → quipu

txtai auto-builds a semantic graph from embedding similarity, runs community
detection for topics, and traverses graph paths for GraphRAG. Our graphs are
declared/structural only — quipu has Louvain/PPR/impact but only over asserted
facts; bobbin already *computes* near-duplicate clusters (`bobbin similar
--scan`) and hot topics but throws the edges away. Proposal: emit
`similar_to` chunk edges (bobbin's `chunk_edges` table already exists) and,
where the knowledge feature is on, mint them as quarantined low-trust facts —
similarity is a model judgment and must not masquerade as observation. That
gives context assembly a semantic-neighbor leg alongside the structural and
coupling legs, which is GraphRAG in our vocabulary.

### G6. Embedding model provisioning — caboodle

txtai's killer onboarding property is `pip install` + auto-download defaults.
Quipu deliberately bundles no model (`--features onnx` supplies the runtime
only; `NO_PROVIDER_HELP` explains a three-part manual setup), and camayoc's
own competency scorer sits with tokenizer-but-no-weights after a CDN 403.
The stack's answer shouldn't be silent auto-download — it should be caboodle:
a plan line that provisions checksum-pinned embedding model artifacts for
quipu (and bobbin's custom-model slot), proven by an embed→search round-trip
with a negative control, recorded in state like every other install step.
First-hour experience is caboodle's charter; this is squarely its job.

### G7. Python client bindings — quipu

txtai is Python-native with JS/Java/Rust/Go bindings and an OpenAI-compatible
API; quipu's surface is Rust/HTTP/MCP/CLI and its feature matrix already
lists Python bindings as 🔜. A thin typed client over the REST surface
(requests + dataclasses, generated or hand-kept against
`docs/book/src/reference/rest-api.md`) would open quipu to the data-science
ecosystem txtai lives in — notebooks, evaluation scripts, camayoc's own
Python producers (which currently hand-roll HTTP).

## Watch — real differences, not worth moving on yet

- **Learned sparse embeddings** (txtai's sparse ANN / SPLADE-class models).
  Bobbin's BM25 lane is strong; revisit only if eval shows recall misses that
  reranking (G1) doesn't fix.
- **Index sharding / API cluster.** Quipu federation is read-side with label
  composition, which is the governed version of the same need. Replication
  stays 🔜 until a workload demands it.
- **Cloud index sync.** Bobbin indexes are rebuildable from source; quipu has
  qpack + share lineage + Garage cold copies. Different mechanism, need met.
- **SQL-with-`similar()` surface.** `hybrid_search` (SPARQL ∩ vector) covers
  the use case in our query language; adding SQL would be a second dialect.
- **MLflow-style per-component tracing.** Bobbin's feedback lineage and the
  metrics spools cover the operational questions; trace UIs are nice-to-have.
- **Workflow scheduling and execution** (txtai's cron-scheduled workflows).
  Shuttle v1 deliberately ships no daemon, queue, or scheduler — agents
  advance runs, and its own deferral list (HTTP/MCP server, write-gate
  signature enforcement) is on its tracker. If a production workload ever
  needs scheduled runs, the answer is a thin driver that *advances shuttle
  runs* on a timer (cron, a shantytown role), not a second engine. Revisit
  only when shuttle's "first production workload" exists to measure against.

## Non-gaps — absent by design, and should stay absent

- **LLM inference / RAG answer synthesis in the store.** "Not an LLM framework
  — it doesn't call LLMs; agents call it" (quipu vision doc). Generation in
  the retrieval layer would also break camayoc's observed/inferred boundary.
- **An in-process pipeline/agent framework.** txtai's agents-with-bash-tools
  are a small Claude Code, and its workflows are unsigned callables in one
  Python process. The stack already has a workflow engine — shuttle — and it
  is deliberately the *inverse* shape: it owns the governed record (declared
  state machines, signed transitions, freezable history in quipu) and refuses
  to be the executor. Execution stays with agents and harnesses (shantytown,
  creel, skein), where the store can't be steered by its own content. Porting
  txtai's executor shape into shuttle would erase its reason to exist.
- **Audio/video/speech/OCR multimodal.** No deterministic parser exists, so
  under ingress rule 3 the output would be inference wearing observation's
  clothes. If ever needed, transcripts arrive as *documents* (G3) produced
  outside the stack, tagged with their real provenance. Bobbin's
  `multimodal.rs` keeps the captioning seam explicitly for later.
- **Model training / fine-tuning / ONNX export pipelines.** Out of every
  repo's charter; models are consumed as pinned artifacts (see G6).

## Suggested next steps

1. File each of G1–G7 as a bead in its home repo (bobbin ×3, camayoc ×1,
   caboodle ×2, quipu ×1), pitch-labelled where the repo uses the pitch flow,
   with this document as the rationale link.
2. G1 and G5 should land with eval evidence from bobbin's harness before they
   default on — a ranking change without a measured win is noise.
3. G6 belongs on caboodle's roadmap next to the existing model-less quipu
   verify path, which currently proves retrieval without ever proving
   embedding.
