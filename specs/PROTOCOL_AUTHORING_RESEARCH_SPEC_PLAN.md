# Spec/Plan: Foretias Protocol Authoring — Research Project

- [ ] proposed — awaiting user review

**Scope:** Major — research project (no production code written by this
project; specification and planning only at the time of approval).
**Status:** Draft proposal. Corpus and plan listed by URL / name. Nothing
downloaded yet. No directories created yet. User to confirm scope before
Phase 1 begins.
**Output:** Research report + style guide + recommended Foretias Protocol
document family + templates — **NOT** the Foretias Protocol itself.
Authoring the actual Foretias Protocol corpus is a separate future Major
informed by this project's output.

---

## Part 0 — Why this exists

Foretias will eventually need a published protocol corpus comparable in
role to the Bitcoin whitepaper + Ethereum Yellow Paper + EIPs + devp2p
specs, or to the Bluetooth Core Specification + profiles, or to the USB
Base + Class specifications. The *technical* content of that corpus is
being worked out elsewhere in `foretias/specs/`. This project addresses
the orthogonal question:

> Given what Foretias is, how should its protocol documents be **written**
> — structured, phrased, layered — to maximize the chance of broad
> adoption?

Adoption is the optimization target. Style decisions (single document vs.
family; formal-math vs. prose; RFC-style imperatives vs. narrative;
versioned editions vs. living spec; paywalled vs. open; certified test
suite vs. honor system) demonstrably affect uptake. Bitcoin, Ethereum,
TLS, QUIC, USB, Bluetooth, PCIe, Wi-Fi all chose differently and all
succeeded; WS-\*, XHTML 2.0, and OpenPGP-as-deployed are well-known
cautionary cases. Without a systematic survey, the Foretias Protocol
corpus will default to the style of whichever exemplar the author last
read.

### Why this is its own project (not part of authoring)

The two skills are different. Studying how successful protocols are
*written* — including reading their reference implementations to see how
the writing held up — is a research task. Using that research to author
the Foretias corpus is an authoring task. Splitting them lets the
research stage be reviewed and corrected before any load-bearing prose
gets committed to the Foretias public record.

---

## Part 1 — Research axes

The research is organized along four families of questions. Every corpus
entry is examined against all four.

### 1.A — How protocols communicate *small things*

Bit-level, byte-level, message-level definitions. How does the spec
make implementations byte-for-byte interoperable?

- **Bit layout** — bit numbering convention (MSB-first / LSB-first),
  packed-field encoding, alignment, padding, reserved bits.
- **Byte layout** — endianness convention and where it's declared;
  word size; signed-vs-unsigned conventions.
- **Framing** — message boundaries, length-prefix vs. delimiter vs.
  fixed size; escape sequences; MTU and fragmentation rules.
- **Message types** — opcode / tag conventions; type registries;
  reserved / future-use markers.
- **Identity encoding** — peer / node / account / device identity:
  raw bytes, hex strings, base58, base64, custom; key-derived vs.
  assigned; collision policy.
- **Cryptographic primitives on the wire** — signature format,
  hash format, MAC format — by algorithm OID, by named suite, by
  raw bytes?
- **Time representation** — Unix epoch, NTP, ISO 8601, monotonic
  counter, slot number, chronon index?
- **Address representation** — IP, multiaddr, peer-id, DID, custom;
  text form, binary form.
- **Presentation style for wire format** — ASCII packet diagrams,
  ABNF, ASN.1, Protobuf, RLP, SSZ, hand-drawn boxes-and-fields,
  code-as-spec.

### 1.B — How protocols communicate *large things*

System-level guarantees, assumptions, and concerns. How does the spec
make the *behavior* of the system precise — and how does it tell readers
what the system promises and requires?

- **Consensus / agreement** — what's promised, under what conditions,
  with which fault thresholds. Verbatim phrasings collected.
- **Liveness** — eventual progress under stated network model.
- **Safety** — no two honest parties commit incompatible outputs.
- **Fault tolerance** — crash, omission, Byzantine, fail-stop;
  thresholds; recovery.
- **Network model** — synchronous, partially synchronous,
  asynchronous; bounds on Δ; what happens during partitions.
- **Stability** — congestion behavior, backpressure, retry storms,
  rate limiting; how the spec keeps the system from melting down at
  scale.
- **Capacity claims** — throughput, latency, scaling; conditions
  attached.
- **Adversary model** — Dolev-Yao, computational, rational,
  active-vs-passive; capabilities and limits.
- **Threat model document placement** — top of spec, separate
  companion, distributed throughout, none.
- **Trust roots and transitivity** — what is trusted, how that trust
  is bootstrapped, how it is rotated.
- **Economic / incentive layer** (where applicable) — when behavior
  is enforced by cost rather than by impossibility.

### 1.C — How protocols serve their systems and their systems' *users*

Specs are not only for compilers. They serve a community of
implementers, integrators, operators, vendors, and end users. How does
the writing serve each?

- **Implementer service** — onboarding path, quickstart, minimal
  conforming implementation guide, worked examples, test vectors,
  reference implementation policy.
- **Integrator service** — API surface document, language-binding
  conventions, library ecosystem signals.
- **Operator service** — observability requirements, debugging
  facilities, error taxonomy, deployment guidance, capacity planning.
- **Vendor service** (for hardware specs) — certification program,
  vendor-ID registry, plug-fests, conformance labs, logo program.
- **End-user service** — what does the spec say (if anything) about
  privacy, reliability, cost, recoverability, ownership of data?
- **Governance service** — how the community feels ownership;
  membership, voting, public review periods, RFC author credit.
- **Newcomer service** — does a stranger to the protocol find a clear
  entry door?

### 1.D — How protocols communicate *themselves* (the meta layer)

The spec-about-the-spec — the choices that make the protocol corpus
itself navigable and durable.

- Audience signaling per document.
- Document family layout and cross-referencing.
- Levels of detail / layering boundaries.
- Normative phrasing convention (RFC 2119 MUST/SHOULD/MAY etc.).
- Formalism choices: prose / pseudocode / state machine / math /
  executable spec.
- Profile / tier / option definition mechanism.
- Versioning, deprecation, sunset policies.
- Amendment / improvement process document.
- Conformance and test-vector publication.
- Reference-implementation policy.
- Errata channel.
- License and IP policy.
- Terminology discipline (glossary placement, new-term introduction).
- Diagram style and tooling.
- Length per layer.
- Onboarding artifacts (quickstart, tutorial, video, blog).
- Explicit out-of-scope statements.

---

## Part 1.5 — Known Foretias characteristics (the compare-and-contrast lens)

While reading every corpus entry, the researcher carries the following
short roster of *what Foretias already is*. The goal is not to write the
Foretias Protocol document (that's the next Major) but to read actively:
"what does this protocol do here, what does Foretias do in the
equivalent place, what could Foretias adapt?"

Source: `wdocs/README.md` (legacy FOSITAS) and
`wdocs/whitepaper/whitepaper.md` (current Foretias v0.2), plus the
existing `foretias/specs/` corpus.

### 1.5.A — Mission and human service
- **Problem solved:** prove *when* a piece of content existed. Anti-
  backdating timestamp service.
- **User audiences (named in the whitepaper):** journalists,
  inventors / patent filers, whistleblowers, researchers (pre-
  registration), hospitals / regulated healthcare, legal
  professionals, software supply chain, financial / compliance
  organizations, contract counterparties.
- **API surface, deliberately narrow:** exactly two operations,
  **Stamp** and **Verify**. Listed as a non-goal that there will
  ever be more.
- **End-user delivery channels (target):** browser extension, CLI,
  REST API, language libraries (Rust / Python / Java).
- **Stated philosophy:** "temporal integrity as a fundamental right."

### 1.5.B — Cryptographic guarantees
- **Unconditional core property:** a destroyed private key cannot be
  used to sign a past chronon. Backdating is structurally impossible,
  not merely expensive.
- **Conditional property (network availability):** verification
  requires that *some* node still serves the relevant chrononchain
  segment. Framed in the whitepaper as a P2P-availability question,
  comparable to BitTorrent / IPFS / Syncthing / Matrix.
- **Classical stack:** Ed25519 (EdDSA over Curve25519) for signing,
  BLAKE3 for fingerprinting, SHA-256 for standards compatibility,
  ChaCha20-Poly1305 for encrypted storage at rest. (libsodium.)
- **Post-quantum stack:** SPHINCS+ (stateless hash-based, NIST
  2024) and ML-DSA / Dilithium (lattice-based, NIST 2024). Long-
  horizon Foretis records use PQC. (liboqs.)
- **Threshold signing:** FROST (Flexible Round-Optimized Schnorr
  Threshold Signatures) for epoch / committee attestations — no
  single participant holds the full key.
- **Custom-plugin / crypto-server abstraction:** all cryptographic
  primitives sit behind a narrow service layer. This is the seam
  along which a future hardware backend (enclave / TPM-like / custom
  silicon) plugs in without changing the host protocol. Per project
  rules, this seam is described abstractly in public docs.

### 1.5.C — P2P and system-level guarantees
- **Transport:** libp2p (same library as Ethereum, IPFS, Filecoin).
- **Discovery:** Kademlia DHT, multi-purpose (peer discovery,
  capability matching, TBID resolution).
- **Calendar replication:** chrononchains are designed to be
  replicated across independent Time Beings. More replicas → closer
  to the unconditional cryptographic guarantee in practice.
- **Calendar active mirroring and proof-of-storage** (in-progress
  spec groups 4b / 4c) — how replication is requested, served, and
  challenged.
- **Mutual attestation between Time Beings** (group 6, large spec
  recently merged) — cross-signed key transitions.
- **Communerdette / capability matching** (group 7, recently
  merged) — peer-to-peer service discovery layer.
- **Probity gossip, epoch consensus, identity-collision detection**
  — phased work in the existing P2P spec family.
- **Vindictive self-policing:** stated network property — challenged
  Time Beings respond with proportionate challenges. Stable, honest
  network through reciprocal escalation rather than central authority.

### 1.5.D — Identity and lifecycle
- **Per-run identity:** process start = brand-new identity; process
  exit = identity destroyed; restart = unrelated new identity. No
  persistent identity continuity across runs (with one explicit
  carve-out for dormant verification-only mode).
- **Identity collision = dual termination:** if two running entities
  hold the same `tbid` / PeerID (RNG failure, VM clone), both go
  dormant (answer queries, refuse to stamp).
- **TBID:** Time Being identifier, derived from key material; the
  unit of address in the DHT.
- **Latin namespace** (informative): Time Being = *Chronos fidelis*;
  Time Family = *Chronos adunatrix*; Chronomatter = *Chronos
  authenticus*; Calendar = *Chronos graphus*.

### 1.5.E — Trust and governance posture
- **No Foretias organization, no master key, no administrator.**
  Authority is mathematical, not institutional.
- **"No office to subpoena"** — explicit positioning against
  jurisdictional capture.
- **Free and open source**, NIST-standardized primitives, auditable
  end-to-end.
- **Reuse of legitimate stacks** as a deliberate adoption tactic:
  "the same libp2p as Ethereum," "Kademlia as in BitTorrent and
  Ethereum," "NIST-standardized PQC." This is itself a *spec-writing*
  choice the researcher should notice when other protocols do it.

### 1.5.F — Implementation discipline
- **Rust** node layer + **C11** verified cryptographic core, linked
  through a narrow auditable FFI.
- **Three language bindings:** Rust native, Python (PyO3), Java
  (JNI). Surface area kept intentionally minimal (Stamp + Verify).
- **Spec-driven development:** the `foretias/specs/` directory is
  the authoritative source of truth; implementation aligns to spec,
  not the reverse.

### 1.5.G — Planned service tiers (relevant to hardware-spec study)
- **Software tier:** what currently exists. Crypto in libsodium /
  liboqs; storage on local disk; peering via libp2p.
- **Hardware tier (future):** custom-plugin backend that may run in
  an enclave, TPM-like device, or dedicated silicon. The protocol
  must specify the *interface* such that software and hardware
  backends are observationally interchangeable — exactly the kind
  of multi-tier specification problem USB / Bluetooth / TPM /
  NVMe solve.

### 1.5.H — Explicit non-goals (the whitepaper says these directly)
- Foretias does **not** judge content (it witnesses fingerprints).
- Foretias does **not** store content (only fingerprints on the
  chrononchain).
- Foretias does **not** provide sub-minute precision (composable
  with RFC 3161 if needed).
- Foretias does **not** replace legal instruments (notarization,
  certified registry entries).
- Foretias has **exactly two services** — adding a third is itself a
  scope decision, not a default.

### 1.5.K — Developer community posture, tone, and machine readability

**Foretias is primarily a library / computing system used by coders.**
The Stamp / Verify surface is consumed by application code (Rust,
Python, Java bindings) — and increasingly by AI coding agents that
write that integration code. Non-developer stakeholders
(journalists, hospitals, whistleblowers, regulators) read the
whitepaper and possibly a quickstart; they do not read the protocol
corpus. Tone decisions for the protocol corpus must therefore
optimize for the **developer audience first** and the **non-developer
audience second** via a deliberately distinct whitepaper voice.

The developer audience itself has three distinct sub-audiences that
the tone must serve simultaneously:

1. **Human application developers (integrators).** Want to depend on
   Foretias, write code that calls Stamp / Verify, and not become
   experts. Reference points: gRPC, Protobuf, CBOR docs.
2. **Human protocol implementers and cryptographic reviewers.** Want
   to write a new Foretias node, audit the crypto, or interoperate.
   Reference points: TLS 1.3 RFC 8446, Noise, Signal specs.
3. **AI coding agents.** Increasingly write the integration code for
   audience (1) and increasingly review specs for audience (2).
   Their needs overlap with humans but with sharper requirements.

#### Tone (axes the style guide will eventually decide)

- **Mission-serious, never grave.** Bitcoin-whitepaper matter-of-
  fact, not corporate-formal (USB-IF, Bluetooth SIG) and not
  bro-casual.
- **Anti-corporate, not anti-professional.** Reference points:
  libp2p, Rust RFCs, Matrix — professional-but-community-led.
- **Precise enough for cryptographers, accessible enough for
  integrators.** Crypto sections at RFC 8446 level of care;
  integration sections at gRPC level of approachability.
- **Reciprocally welcoming to adjacent communities.** Foretias
  reuses libsodium, libp2p, liboqs, FROST — their contributors are
  natural Foretias contributors. The tone signals "we are part of
  this ecosystem."
- **Mistakes are public learning, not silent corrections.** Errata
  posture is part of the trust story.
- **Code of conduct.** Foretias serves a user base that includes
  vulnerable users; a clear, enforced CoC is part of the trust
  posture, not virtue-signal. How it's referenced from the spec
  matters.
- **Pronouns and voice.** Choice between "this specification…",
  "we…", "you…", "an implementation MUST…" is a tone decision that
  the style guide will settle once and enforce. The research
  catalogs what successful primitives chose.
- **Humor and personality.** Minimal in normative documents; room
  in narrative whitepaper and contributor guides. Validate against
  the corpus.
- **International audience; English-language craft is a minor
  input.** Most successful protocols (USB, Bluetooth, IETF RFCs,
  Ethereum, libp2p) are used internationally, by implementers whose
  first language is not English. English phrasing patterns
  (MUST/SHOULD/MAY, voice, sentence rhythm) are worth cataloging for
  the style guide but are not a major adoption lever — translation
  discipline, ASCII-portable wire formats, language-agnostic
  examples, locale-free identifiers, and clear non-idiomatic prose
  matter more. The research notes specific phrasing patterns
  observed; the style guide treats them as one tool among many
  rather than as a load-bearing decision.
- **Fair-and-balanced posture (disclaimers, limitations, caveats).**
  Does the document name conditions under which the protocol fails?
  ("Bitcoin won't work if Earth is hit by a planet-scale EMP" is
  the comic-extreme version; "Bitcoin's security degrades if more
  than 50% of hashpower colludes" is the real one.) Foretias's
  whitepaper already models this: Section 5 explicitly names the
  network-availability dependency, compares to BitTorrent's 38%
  first-month dropoff, and warns that an unreplicated chrononchain
  carries real risk; Section 6 enumerates what Foretias does *not*
  do. The research **observes** how each corpus member handles this
  rather than prescribing presence — possibilities include RFC
  "Security Considerations" sections, "Known Issues," "Limitations,"
  academic "Caveats," composed-with comparisons that don't
  strawman alternatives, *or no disclaimers at all* when that is the
  pervasive style of the genre (some hardware specs, some
  marketing-adjacent whitepapers, some terse primitives). Absence is
  data: if a genre routinely omits failure conditions, that's a
  finding about the genre, not a defect of a particular spec. The
  Foretias style decision — whether to follow the genre or
  deliberately depart from it — falls out of this observation. The
  current whitepaper has already chosen "depart from
  marketing-whitepaper convention by being honest about failure
  modes"; the protocol corpus will inherit a similar choice
  consciously rather than by default.

#### Community infrastructure visible from the spec

Tone is reinforced (or undermined) by what the spec links to:

- Public discussion channels — mailing list, Matrix room, GitHub
  Discussions, IRC. How are they introduced?
- Open-meeting cadence if any.
- Contributor onboarding doc.
- Code of Conduct reference and enforcement description.
- Acknowledgments / authors section — style, size, categories.
- IP / CLA posture — invitation or deterrent?
- Public roadmap.
- "How to propose a change" workflow.
- Where to ask "I'm confused" questions without embarrassment.
- Public test-suite scoreboard if any.

#### Machine and AI-coding-agent readability

A spec that AI coding agents can read accurately is a spec that gets
integrated faster — because agents will write more integration code
year over year, and a misread spec produces broken code at scale.
Concrete requirements the Foretias Protocol corpus will likely need:

- **Stable, citable section anchors** so a tool can quote
  "PROTOCOL_FAMILY 4.3.2" and a reader (human or agent) can verify.
- **Self-contained sections.** A section should be quotable without
  pulling the whole document into context. Avoid pronouns that
  depend on three pages back.
- **Machine-extractable schemas alongside prose.** OpenRPC for the
  API surface; protobuf / CBOR schemas for messages; ABNF or
  formal grammar for wire format; CDDL for CBOR data definitions.
  The prose explains intent; the schema is unambiguous.
- **Test vectors as files**, not just embedded hex in prose. JSON
  or CBOR fixtures that an agent (or test harness) can load and
  diff against an implementation's output.
- **Worked examples that show idiomatic use.** Agents pattern-match
  on examples; an example is worth ten paragraphs of prose for
  shaping the code an agent will write.
- **Disambiguation of common mistakes.** "An implementation MUST
  NOT … because otherwise …" sections that pre-empt the wrong
  inferences an agent (or human) would otherwise draw.
- **Length budgets that fit reasonable context windows.** A
  500-page single-document spec means agents see only the chunk
  they retrieve; multi-document corpora with clean cross-references
  retrieve better.
- **Permissive, unambiguous license on spec text** so agents
  trained on or quoting from the spec have no friction.
- **Versioned URLs and `latest`-pointer discipline** so an agent
  citing the spec today doesn't get a silent-rewrite tomorrow.
- **No load-bearing diagrams.** A diagram that *only* exists as a
  PNG is invisible to text-only retrieval; pair every diagram with
  a textual description or ASCII rendering.

The research must catalog which corpus members already do these
things (Ethereum execution-specs is executable Python, which agents
read perfectly; OpenRPC is machine-consumable by design; RFC 8949
CBOR ships with CDDL schemas) and which don't, then recommend a
Foretias baseline.

### 1.5.J — Position in the protocol stack (the "common denominator")

Foretias is deliberately positioned as a **primitive layer** — a "GCD"
of modern P2P systems. It sits:

- **Above** libraries that already exist and are reused: libp2p
  (transport, peer discovery, gossipsub), Kademlia (DHT), libsodium
  (classical crypto), liboqs (post-quantum), FROST implementations.
  Foretias **uses** these protocols rather than reinventing them.
- **Below** application-feature systems like Bitcoin, Ethereum,
  Filecoin, IPFS-as-product. Those systems add ledgers, smart
  contracts, value transfer, content addressing, naming, marketplaces.
  Foretias does not. A Bitcoin- or Ethereum-like project could in
  principle *use* a Foretias network as its timestamp authority
  without adopting Foretias's other choices.
- **Beside** narrowly-scoped time-stamping primitives like RFC 3161
  (centralized TSA) and OpenTimestamps (Bitcoin-anchored). Foretias
  is the same kind of artifact — a single-purpose timestamp service
  — but with a different trust model (destroyed-key cryptography +
  P2P availability instead of CA trust or proof-of-work anchoring).

**Consequences for spec authoring** that the research must inform:

1. **Primary audience = integrators**, not end users. Documents like
   TLS, libp2p, gRPC, Protobuf, CBOR succeed because integrators can
   wire them in confidently. End-user docs are secondary (browser
   extension, CLI) and live in a different document with a different
   voice.
2. **Layered-dependency documentation style**. Primitive-layer specs
   typically declare what they sit on (TLS 1.3 cites cipher
   primitives by reference; QUIC 9000 layers explicitly on UDP and
   companion RFCs for crypto/recovery; gRPC layers on HTTP/2). The
   Foretias Protocol must do the same for libp2p, libsodium, liboqs,
   FROST — saying clearly *we depend on these, here's the version,
   here's the surface we use, here's what we add on top*.
3. **Profile-narrow surface**. GCD protocols expose a small, focused
   API (Stamp / Verify) and let others compose. The spec's tone is
   "here's a primitive, here's its precise contract" rather than
   "here's a platform."
4. **Composability statements**. The spec should explicitly describe
   how Foretias composes with the layers above (a Bitcoin-style ledger
   could use Foretias for timestamping; an enterprise audit pipeline
   could anchor logs in Foretias) and below (Foretias does not
   re-specify libp2p; it specifies how it uses libp2p).
5. **Non-poaching discipline.** The Foretias spec must resist
   restating what libp2p / libsodium / liboqs already state. Each
   reference rather than restatement is a Foretias-Protocol style-
   guide rule the research will validate against successful primitives.

### 1.5.I — How to use this lens during reading

For every corpus entry, the researcher carries 1.5.A–H in their head
and writes (in the rubric's "Compare to Foretias" subsection):

1. **Equivalent place:** where in this protocol is the analog of a
   Foretias element from 1.5.A–H?
2. **Different choice:** if the analog exists but differs, what is the
   difference and why?
3. **Adoption candidate:** is there a phrasing, layering, profile
   mechanism, or governance practice from this protocol that Foretias
   could adapt? One concrete sentence; no commitment.
4. **Counter-pattern:** is there anything this protocol does that
   Foretias should *not* adopt, and why? (Counter-patterns are as
   useful as patterns.)

This active-comparison habit is what turns the corpus survey into
useful input for the eventual Foretias Protocol corpus, instead of an
encyclopedia exercise.

---

## Part 2 — What to look for while reading

Two checklists. The researcher uses them when reading each corpus
entry's spec and (selectively) its reference implementation. Each item
expects the researcher to quote 1–3 verbatim passages plus a 1–2
sentence pattern note.

### 2.A — Spec reading checklist

**Small-things layer (axis 1.A)**

- How is byte order declared, and where exactly (section number, chapter)?
- How is bit numbering shown in diagrams?
- What is the smallest atomic unit (bit, octet, word)?
- How are variable-length fields handled — length-prefix, terminator,
  fixed-tag, TLV?
- How is identity (peer / node / account / device) encoded? Bytes? Hex?
  Base58? Custom?
- How are signatures, hashes, MACs specified — by algorithm OID, by named
  suite, by raw bytes? Where does the registry live?
- How is time represented?
- How is the wire format presented — packet diagrams, ABNF, ASN.1,
  schema, code?
- How are reserved / future-use fields described?
- How are alignment and padding rules stated?
- Are there test vectors that pin the encoding bit-for-bit?

**Large-things layer (axis 1.B)**

- What guarantees does the protocol promise? Quote them verbatim.
- What assumptions does the protocol require? Quote them verbatim.
- How is the adversary defined? Where?
- How is the network model defined (sync / async / partial)?
- How is liveness phrased (if at all)?
- How is safety phrased (if at all)?
- How is the fault threshold phrased — positive condition
  ("if honest majority…"), adversary bound ("at most ⌊n/3⌋…"),
  defined term ("an honest validator is one that…"), formal predicate?
- How is network stability addressed — congestion control,
  backpressure, retry policy, rate limits?
- Where is the threat model — top of doc, separate companion,
  distributed throughout, absent?
- Is there a proof, a proof reference, or only assertion?
- Are tradeoffs explicit? (CAP, latency-vs-throughput,
  decentralization-vs-cost.)

**System / user-service layer (axis 1.C)**

- How does the doc onboard a new implementer (quickstart, walkthrough,
  tutorial)?
- What worked examples are present (handshake trace, sample messages,
  test vectors with expected output)?
- How does the spec describe debugging and observability?
- How are error conditions enumerated? Are error codes registered?
- How does the spec describe operator concerns (capacity, monitoring,
  deployment)?
- How does the spec describe end-user concerns, if at all?
- How does the spec describe upgrade and migration paths?
- How is privacy / data handling described?
- Is there a "Security Considerations" section? How long, how serious?
- For hardware specs: how is a certification path described?

**Stack-position layer (axis 1.5.J — active reading lens)**

For every corpus entry, the researcher answers:

- Where does this protocol sit in its own ecosystem stack? Primitive
  (TLS, Noise, Protobuf, CBOR), common-denominator (libp2p, gRPC),
  application (Bitcoin, Ethereum, Matrix), or vertically-integrated
  (USB-the-device-class-on-USB-the-base)?
- Which lower layers does it explicitly cite vs. restate? (Reference
  discipline is itself a spec choice.)
- Which upper layers compose with it? Is that composition shown by
  worked example, by reference, or left implicit?
- How does the chosen position show up in the writing — voice
  (primitive: "here is the precise contract"; application: "here is
  what you can build"), audience signaling, length, profile mechanism?
- Concretely: if Foretias adopted this protocol's spec style, would
  it still read as a primitive, or would it accidentally read as an
  application?

**Meta layer (axis 1.D)**

- Audience statement at start of doc.
- Cross-references to companion documents.
- Glossary placement.
- Diagram count and style.
- Length per layer (page count or word count).
- Normativity markers.
- Versioning policy quoted.
- Amendment-process document referenced.
- License / IP statement quoted.
- Out-of-scope statements quoted.

**Tone, community, and machine-readability observation pass (per
1.5.K).** Note: voice (third-person passive / "we" / "you" / "an
implementation MUST"); seriousness vs. warmth; corporate-formal vs.
community-led; how welcoming to newcomers; presence/style of
acknowledgments, contributor onboarding link, CoC reference, public
discussion channel; machine-readability features (stable section
anchors, machine-extractable schemas, test-vector files, OpenRPC /
CDDL / ABNF / protobuf companions, license clarity, length per
chunk, diagram/text pairing).

**Fair-and-balanced observation pass.** Note: are limitations,
failure conditions, caveats, "Security Considerations," "Known
Issues," or honest comparisons with alternatives present? Quote 1–2
examples. **If absent, note whether the genre routinely omits them**
(e.g., short hardware primitives, marketing whitepapers, terse RFCs)
— absence is data, not a defect.

**Foretias-lens pass.** After working through the checklist for a
corpus entry, the researcher does one short pass through Part
1.5.A–K asking: "for each Foretias element, where is the analog in
this protocol, and what would Foretias adopt or avoid?" Quotes go
into the rubric's `Compare to Foretias` subsection (Part 3) and,
when about phrasing of guarantees, into
`PHRASING_COOKBOOK_DRAFT.md`.

### 2.B — Reference-implementation reading checklist

When the spec is ambiguous, or when a particular pattern matters for
Foretias, the researcher spends 30–90 minutes in the canonical reference
implementation. The point is not to learn the implementation; the point
is to see *where the spec held up and where it didn't*.

- What spec section is being implemented?
- Does the code's observable behavior match the spec text? Where it
  diverges, why?
- What does the code do that the spec doesn't say? (Implementation
  freedom that should have been spec'd? Or genuine implementation
  freedom?)
- Are there comments referencing the spec, errata, or implementation
  notes ("the spec says X but Y…")?
- How is the code structured around the spec's layering? One module
  per spec section, or different decomposition?
- What test fixtures encode the spec? Are they published and shared
  across implementations?
- For protocols with multiple independent implementations: do two
  implementations decompose the spec the same way? What does the
  difference reveal about the spec?

### 2.C — Activities the researcher performs per protocol

A repeatable minimum. For each protocol the researcher will:

1. Read the headline document (whitepaper or main spec) end-to-end.
2. Read the table of contents of every companion document.
3. Read the wire-format / message-format section in detail (axis 1.A).
4. Read the guarantees / security / fault-model section in detail (1.B).
5. Read the amendment / governance document, even briefly (1.D).
6. Read three improvement proposals or change records as worked
   examples of how the corpus actually evolves.
7. Fill in the per-document rubric (Part 3).
8. Add at least three verbatim quotes to the running
   `PHRASING_COOKBOOK_DRAFT.md` (1.B-related quotes especially).
9. For at least one in five protocols (researcher's choice, weighted
   toward most-cited exemplars), spend 30–90 minutes in the reference
   implementation per checklist 2.B.
10. **Foretias compare-and-contrast pass.** Using the Part 1.5 roster,
    write the rubric's `Compare to Foretias` subsection: equivalent
    place, different choice, adoption candidate, counter-pattern. Two
    to four sentences per axis (1.5.A–H) — short and concrete,
    citing specific spec sections of the studied protocol.

These activities are what makes the research *thorough* rather than
skim-level. They are explicitly enumerated so that sub-agent offload
(Part 6) can be assigned with the activity list as the work
specification. Sub-agents are explicitly briefed with Part 1.5 so the
compare-and-contrast pass happens at survey time, not retroactively.

---

## Part 3 — Per-document rubric

For each corpus entry the researcher (or sub-agent) produces a single
markdown file using this template:

```
Protocol: <name>
URL(s): <links>
Acquired: <local path or "URL only — paywalled">
Years deployed: <int>      Independent implementations: <int>
Ecosystem scale: <one sentence>

# Axis 1.A — Small things
 1. Byte order declaration (quote, section ref):
 2. Bit numbering convention:
 3. Atomic unit:
 4. Variable-length encoding:
 5. Identity encoding:
 6. Signature / hash / MAC encoding:
 7. Time representation:
 8. Address representation:
 9. Wire-format presentation style:
10. Reserved / future-use convention:
11. Alignment / padding rules:
12. Test vectors present? Where?

# Axis 1.B — Large things
13. Guarantees (quote):
14. Assumptions (quote):
15. Adversary model:
16. Network model:
17. Liveness phrasing (quote):
18. Safety phrasing (quote):
19. Fault threshold phrasing (which template; quote):
20. Stability / congestion handling:
21. Threat model placement:
22. Proof present, referenced, or absent:
23. Stated tradeoffs:

# Axis 1.C — System and user service
24. Implementer onboarding:
25. Worked examples present:
26. Debugging / observability:
27. Error taxonomy:
28. Operator concerns:
29. End-user concerns:
30. Upgrade / migration path:
31. Privacy / data handling:
32. Security Considerations section (length, depth):
33. Certification path (hardware specs):

# Axis 1.D — Meta
34. Audience statement (quote):
35. Document family layout:
36. Cross-reference style:
37. Levels of detail / layer list:
38. Normative phrasing convention:
39. Formalism level:
40. Profile / tier definition:
41. Versioning / deprecation policy (quote):
42. Amendment process link:
43. Conformance / test vectors policy:
44. Reference implementation policy:
45. Errata channel:
46. License / IP statement (quote):
47. Glossary placement:
48. Diagram count, style, tool:
49. Length per layer:
50. Onboarding artifacts:
51. Out-of-scope statements (quote):

# Notable patterns (3–5 sentences):

# Tone and community posture (per 1.5.K)
 Voice ("this specification…" / "we" / "you" / "MUST/SHOULD"):
 Seriousness vs. warmth:
 Corporate-formal vs. community-led:
 Welcoming to newcomers (quote opening or contributor invitation):
 Acknowledgments style, size, categories:
 Contributor onboarding link from spec:
 Code of Conduct reference:
 Public discussion channel referenced in spec:
 Pronouns and humor observations:

# Machine and AI-coding-agent readability (per 1.5.K)
 Stable, citable section anchors:
 Self-contained sections (or pronouns spanning pages):
 Machine-extractable schema present (OpenRPC / CDDL / ABNF /
   protobuf / executable spec):
 Test vectors as files (vs. embedded hex in prose):
 Worked examples idiomatic enough for pattern-matching:
 Disambiguation of common misreadings:
 Length per chunk (fits a reasonable context window?):
 License clarity on spec text:
 Versioned URL discipline:
 Every diagram paired with text/ASCII description:

# Fair-and-balanced presentation (cross-cutting observation)
 Disclaimers / limitations / caveats present? Quote 1–2:
 Failure-condition statements present? Quote 1–2:
 Honest comparison with alternatives (or strawmanned)?
 Open problems acknowledged?
 If absent: is absence the genre convention? (yes / no / partial)

# Stack position of this protocol (per 1.5.J)
 Position (primitive / common-denominator / application / vertical):
 Lower layers cited by reference:
 Lower layers restated (and why noticed):
 Upper layers that compose on top (named in spec or implied):
 How position shows in voice / audience / length / profile mechanism:
 Style transferability to Foretias-as-primitive (would Foretias still
  read as a primitive if it borrowed this style? why/why not):

# Compare to Foretias (per Part 1.5; 2–4 sentences per axis)
 1.5.A Mission and human service:
        Equivalent place / Different choice / Adoption candidate / Counter-pattern.
 1.5.B Cryptographic guarantees:
 1.5.C P2P and system-level guarantees:
 1.5.D Identity and lifecycle:
 1.5.E Trust and governance posture:
 1.5.F Implementation discipline:
 1.5.G Service tiers (esp. hardware):
 1.5.H Non-goals discipline (Stamp/Verify-only):
 1.5.J Stack positioning (does this spec position itself the way
        Foretias should?):

# Top three lessons for the eventual Foretias Protocol document (3–5 sentences):
```

Estimated effort: 0.5–2 hours per entry depending on spec size.

---

## Part 4 — Corpus (names + URLs; nothing downloaded yet)

Each entry: name — URL — why included.

Acquisition happens in Phase 1 after the user approves the list.
Paywalled sources are flagged. Phase 1 will create directories under
`foretias/specs/research/protocol_references/` mirroring the categories
below; see Part 7 for the layout specification.

### 4.A — Blockchain / consensus / federation / P2P (software)

- **Bitcoin whitepaper** — https://bitcoin.org/bitcoin.pdf — 9-page
  informal narrative; the canonical example of how a *small* document
  launched a system.
- **Ethereum Yellow Paper** — https://ethereum.github.io/yellowpaper/paper.pdf
  — gold standard for math-heavy formal semantics in a consensus protocol.
- **Ethereum whitepaper (Buterin)** — https://ethereum.org/en/whitepaper/
  — narrative complement to the Yellow Paper.
- **Ethereum execution-specs** — https://github.com/ethereum/execution-specs
  — executable Python "spec is the code" approach.
- **Ethereum consensus-specs (beacon chain)** —
  https://github.com/ethereum/consensus-specs — heavy formalism;
  explicit honest-validator assumptions.
- **Ethereum devp2p** — https://github.com/ethereum/devp2p — wire
  protocol family; multi-document.
- **Ethereum execution-apis (JSON-RPC)** —
  https://github.com/ethereum/execution-apis — OpenRPC-style API surface.
- **EIP-1** — https://eips.ethereum.org/EIPS/eip-1 — amendment process.
- **BIP-1 / BIP-2** — https://github.com/bitcoin/bips/blob/master/bip-0001.mediawiki
  — Bitcoin's amendment process.
- **libp2p specs** — https://github.com/libp2p/specs — distributed
  multi-document family with maturity tags.
- **IPFS specs** — https://specs.ipfs.tech/ — newer P2P protocol family.
- **Matrix specification** — https://spec.matrix.org/ — federated;
  versioned across client-server / server-server / identity / push.
- **ActivityPub (W3C)** — https://www.w3.org/TR/activitypub/ — W3C
  federated social.
- **SSB protocol guide** — https://ssbc.github.io/scuttlebutt-protocol-guide/
  — informal but comprehensive P2P spec; contrast point to RFC style.
- **Hypercore protocol** — https://hypercore-protocol.org/ — append-only
  log P2P; structurally similar to a chrononchain.
- **Tendermint / CometBFT** — https://docs.cometbft.com/ — explicit
  honest-supermajority phrasing.

### 4.A-bis — Direct positional analogs (primitive-layer time and trust)

**(Added after Foretias's stack-positioning was clarified.)** These are
the most direct *positional* analogs to Foretias — single-purpose
primitive-layer protocols whose audience and structure most resemble
what the Foretias Protocol corpus needs to be.

- **RFC 3161 (Time-Stamp Protocol, TSP)** —
  https://www.rfc-editor.org/rfc/rfc3161 — centralized timestamping
  authority. Direct ancestor in problem space; opposite trust model.
  **First-tier study.**
- **RFC 5816 (ESSCertIDv2)** —
  https://www.rfc-editor.org/rfc/rfc5816 — TSP companion. Study how
  a small primitive is amended.
- **OpenTimestamps** — https://opentimestamps.org/ — Bitcoin-anchored
  decentralized timestamping. Same problem, different trust model
  (PoW anchoring). Spec is informal but adopted.
- **Roughtime (IETF draft)** —
  https://datatracker.ietf.org/doc/draft-ietf-ntp-roughtime/ —
  cryptographically authenticated time (Google / Cloudflare).
  Different problem (current time, not historical witness) but
  primitive-layer kindred.
- **NTS (Network Time Security, RFC 8915)** —
  https://www.rfc-editor.org/rfc/rfc8915 — same.
- **Trillian / Certificate Transparency (RFC 6962, RFC 9162)** —
  https://www.rfc-editor.org/rfc/rfc9162 — append-only verifiable
  logs. Structurally very close to a chrononchain.
- **Sigstore / Rekor transparency log** —
  https://docs.sigstore.dev/ — modern application of the
  transparency-log primitive.
- **gRPC** — https://grpc.io/docs/what-is-grpc/core-concepts/ —
  common-denominator API framework; multi-language; layered on HTTP/2.
- **Protocol Buffers** — https://protobuf.dev/programming-guides/proto3/
  — primitive serialization spec; massively reused.
- **CBOR (RFC 8949)** — https://www.rfc-editor.org/rfc/rfc8949 —
  compact binary encoding; small primitive done well.
- **COSE (RFC 9052)** — https://www.rfc-editor.org/rfc/rfc9052 —
  CBOR Object Signing and Encryption; primitive crypto-on-CBOR.
- **gossipsub (libp2p)** — https://github.com/libp2p/specs/tree/master/pubsub/gossipsub
  — primitive pubsub used by Ethereum / Filecoin / others.
- **CRDT specifications (Yjs / Automerge)** —
  https://docs.yjs.dev/ , https://automerge.org/docs/ — primitive
  data-structure protocols.

### 4.B — Cryptographic / transport protocols (software, IETF-style)

- **TLS 1.3 (RFC 8446)** — https://www.rfc-editor.org/rfc/rfc8446 — IETF
  gold standard for cryptographic-protocol prose.
- **QUIC (RFC 9000)** — https://www.rfc-editor.org/rfc/rfc9000 — modern
  transport with normative companions (9001 crypto, 9002 recovery,
  9221 datagrams).
- **HTTP/3 (RFC 9114)** — https://www.rfc-editor.org/rfc/rfc9114.
- **HTTP/1.1 rewrite (RFCs 9110 / 9111 / 9112)** —
  https://www.rfc-editor.org/rfc/rfc9110 — study the 2022 rewrite of a
  20-year-old spec.
- **Noise Protocol Framework** — https://noiseprotocol.org/noise.html —
  pattern notation, terse, very precise.
- **Signal Double Ratchet** —
  https://signal.org/docs/specifications/doubleratchet/ — algorithmic
  pseudocode with security analysis.
- **Signal X3DH** — https://signal.org/docs/specifications/x3dh/.
- **MLS / Messaging Layer Security (RFC 9420)** —
  https://www.rfc-editor.org/rfc/rfc9420 — group messaging crypto.
- **WireGuard whitepaper** — https://www.wireguard.com/papers/wireguard.pdf
  — 12 pages; deliberately minimal.
- **SSH (RFCs 4251–4254)** — https://www.rfc-editor.org/rfc/rfc4251 —
  multi-RFC protocol family.
- **OAuth 2.1** — https://datatracker.ietf.org/doc/html/draft-ietf-oauth-v2-1.
- **OpenID Connect Core 1.0** —
  https://openid.net/specs/openid-connect-core-1_0.html.

### 4.C — Internet infrastructure (software, massively adopted)

- **IP (RFC 791)** — https://www.rfc-editor.org/rfc/rfc791.
- **TCP (RFC 9293)** — https://www.rfc-editor.org/rfc/rfc9293 — the 2022
  consolidation of RFC 793 + 40 years of errata.
- **BGP-4 (RFC 4271)** — https://www.rfc-editor.org/rfc/rfc4271.
- **DNS (RFCs 1034 / 1035)** — https://www.rfc-editor.org/rfc/rfc1035.
- **SMTP (RFC 5321)** — https://www.rfc-editor.org/rfc/rfc5321.
- **RTP (RFC 3550)** — https://www.rfc-editor.org/rfc/rfc3550.
- **RFC 2119 + RFC 8174** — https://www.rfc-editor.org/rfc/rfc2119 —
  defines MUST / SHOULD / MAY.
- **RFC 2026 + RFC 8729** — https://www.rfc-editor.org/rfc/rfc2026 —
  the IETF process itself.

### 4.D — Hardware-implemented protocols (massively adopted)

These specs directly inform Foretias's planned hardware service tiers.
They demonstrate how to write a single normative corpus that silicon,
firmware, and host-software teams all conform to.

- **USB 2.0 / 3.2 / 4 base** — https://www.usb.org/documents — device
  class profiles (HID, MSC, CDC, video, audio) layered over base spec.
  Quintessential study in tier-and-profile structure.
- **Bluetooth Core Specification 5.x** —
  https://www.bluetooth.com/specifications/specs/ — multi-volume;
  GAP / GATT / profiles. Registration required.
- **PCIe Base Specification** — https://pcisig.com/specifications —
  physical / data link / transaction layers. **Paywalled — members only.**
- **NVMe Base + NVM Command Set + Transport specs** —
  https://nvmexpress.org/specifications/ — deliberately factored;
  recent redesign worth studying.
- **IEEE 802.11 (Wi-Fi)** — https://standards.ieee.org/ieee/802.11/ —
  **Paywalled / IEEE Get-Program for older revisions.**
- **IEEE 802.3 (Ethernet)** — https://standards.ieee.org/ieee/802.3/.
- **TPM 2.0 (TCG)** — https://trustedcomputinggroup.org/resource/tpm-library-specification/
  — Architecture / Structures / Commands / Supporting Routines /
  Platform Profile. **Directly relevant**: security-critical hardware
  spec with optional features and platform profiles.
- **FIDO2 / CTAP2** — https://fidoalliance.org/specifications/ —
  hardware authenticator side.
- **WebAuthn Level 3 (W3C)** — https://www.w3.org/TR/webauthn-3/ —
  paired browser-side spec. Study FIDO2 + WebAuthn together as a
  hardware/software protocol pair.
- **SD Card / eMMC** — https://www.sdcard.org/ , https://www.jedec.org/.
- **I2C-bus specification (NXP UM10204)** —
  https://www.nxp.com/docs/en/user-guide/UM10204.pdf — short, hugely
  adopted; exemplary minimal hardware spec.
- **CAN bus (ISO 11898)** — https://www.iso.org/standard/63648.html —
  vehicle networks. **Paywalled — ISO.**
- **EMV** — https://www.emvco.com/specifications/ — payment terminals;
  multi-book. Free registration.
- **UEFI Specification** — https://uefi.org/specifications.
- **ACPI Specification** — https://uefi.org/specifications.
- **Arm AMBA / AXI / ACE** — https://developer.arm.com/Architectures/AMBA
  — on-chip bus IP; open spec drove enormous adoption.
- **JEDEC DDR / LPDDR** — https://www.jedec.org/standards-documents.
  Free registration; some paywalled.
- **HDMI 2.x** — https://www.hdmi.org/spec — **paywalled** (adopter
  agreement).
- **DisplayPort (VESA)** — https://vesa.org/ — alternative; some specs
  free.
- **Thunderbolt 4** — https://www.intel.com/content/www/us/en/architecture-and-technology/thunderbolt/.

### 4.E — Process / governance / improvement documents

- **RFC 2026 / RFC 8729** — IETF process.
- **EIP-1** — Ethereum Improvement Proposal process.
- **BIP-1 / BIP-2** — Bitcoin Improvement Proposal process.
- **W3C Process Document** — https://www.w3.org/Consortium/Process/.
- **USB-IF Engineering Change Notice procedure** —
  https://www.usb.org/about.
- **TCG Public Review process** —
  https://trustedcomputinggroup.org/specifications-public-review/.
- **IEEE-SA Standards Development Process** —
  https://standards.ieee.org/develop/process/.

### 4.F — Reference cases and counterexamples

- **OpenPGP (RFC 9580)** — https://www.rfc-editor.org/rfc/rfc9580 —
  "how a spec evolved over 30 years and fragmented its implementations."
- **XMPP (RFC 6120)** — https://www.rfc-editor.org/rfc/rfc6120 —
  federated protocol; Matrix-comparison.
- **SOAP / WS-\* family** — https://www.w3.org/TR/soap/ —
  counterexample of over-specification.
- **IPv6 (RFC 8200)** — https://www.rfc-editor.org/rfc/rfc8200 —
  adopted slowly despite excellent specification.

### 4.H — Distributed storage and proof-of-storage / proof-of-spacetime

Foretias has a storage layer: the chrononchain is replicated across Time
Beings (calendar active mirroring, group 4b) and the in-progress group
4c spec adds proof-of-storage. Storage-protocol specs are therefore
directly relevant — both filesystem-style and incentivized-storage
blockchain-style. The proof-of-storage / proof-of-spacetime
constructions in Filecoin, Chia, Arweave, and Storj are particularly
load-bearing for group 4c's design choices.

**Decentralized / incentivized storage chains**
- **Filecoin** — https://filecoin.io/filecoin.pdf (whitepaper);
  https://spec.filecoin.io/ (full spec). Proof-of-replication +
  proof-of-spacetime. **First-tier study** for the proof-of-storage
  question.
- **Chia** — https://www.chia.net/wp-content/uploads/2022/07/ChiaGreenPaper.pdf
  (Green Paper); https://docs.chia.net/. Proof-of-space-and-time.
  Different construction from Filecoin; worth contrasting.
- **Arweave** — https://www.arweave.org/yellow-paper.pdf. Succinct
  random proofs of access; permanent storage model.
- **Storj** — https://www.storj.io/storj-whitepaper.pdf. Reed-Solomon
  + auditing; pragmatic engineering whitepaper.
- **Sia** — https://sia.tech/docs/. Storage contracts; renter/host
  model.
- **Swarm (Ethereum storage layer)** —
  https://www.ethswarm.org/swarm-whitepaper.pdf.

**Classic distributed filesystems (single-organization but
multi-implementation lessons)**
- **Google File System (GFS) paper** (Ghemawat et al., SOSP 2003) —
  https://research.google/pubs/the-google-file-system/. Seminal;
  shaped HDFS, Lustre's modern design, much else.
- **Hadoop HDFS** — https://hadoop.apache.org/docs/stable/hadoop-project-dist/hadoop-hdfs/HdfsDesign.html.
- **Lustre** — https://www.lustre.org/documentation/.
  High-performance parallel filesystem; supercomputing-scale.
- **Ceph** — https://docs.ceph.com/en/latest/architecture/. Object
  store + filesystem + block store; CRUSH paper companion.
- **GlusterFS** — https://docs.gluster.org/.
- **Tahoe-LAFS** — https://tahoe-lafs.readthedocs.io/. Capability-
  based distributed storage with cryptographic erasure-coding;
  unusually well-documented for a research-grade system.
- **MinIO** — https://min.io/docs/minio/linux/index.html.
  S3-compatible object storage; modern docs style.
- **Amazon Dynamo paper** (DeCandia et al., SOSP 2007) —
  https://www.allthingsdistributed.com/files/amazon-dynamo-sosp2007.pdf.
  Eventual-consistency design rationale; influential.

(IPFS is already in 4.A. The 4.A entry covers IPFS-as-protocol-family;
the 4.H Filecoin entry covers the storage-incentive layer built on
top.)

### 4.G — Possibly add (open question for human reviewer)

- ISO 14443 (NFC contactless), ISO 7816 (smart card) — directly
  relevant to hardware-secured identity.
- 3GPP / LTE / 5G specs — massive multi-volume telecom standards;
  the most-adopted hardware/software-layered spec family on earth, but
  enormous.
- ARM Architecture Reference Manual — chip-level instruction set
  specification; relevant if a Foretias hardware tier defines its own
  ISA-like interface.
- POSIX (IEEE 1003) — long-lived behavioral spec.
- gRPC / Protobuf — modern API + IDL specification practice.
- CBOR (RFC 8949) — compact binary encoding spec; small and
  exemplary.

---

## Part 5 — Cross-cutting synthesis questions (Phase 3 output)

Beyond the per-doc rubrics, the synthesis stage must answer:

### 5-A. Median shape of a massively-adopted protocol spec
Number of normative documents, ratio of normative to informative pages,
presence/absence of formal semantics, average length per layer, single
file vs. multi-file family.

### 5-B. Phrasing cookbook for large guarantees
**(User-flagged.)** Specs phrase honest-majority, liveness, safety,
network-synchrony, and capacity claims in characteristic ways. The
cookbook collects verbatim quotes across the corpus, grouped by what
they assert, and identifies the rhetorical templates so Foretias can
pick a deliberate style for each category instead of improvising.

A worked illustration — the kind of artifact this stage produces (the
researcher will verify these specific quotes in Phase 2):

> **Honest-majority assumption.** Bitcoin's whitepaper does *not*
> literally say "unlikely more than 50% are evil." Its phrasing is
> closer to "If a majority of CPU power is controlled by nodes that
> are not cooperating to attack the network, they'll generate the
> longest chain and outpace attackers." It frames the assumption as
> a *positive* condition on honest nodes rather than a *negative*
> bound on adversaries.
>
> PBFT-family specs phrase the same idea as a hard bound: "the
> system can tolerate up to ⌊(n−1)/3⌋ faulty replicas." Tendermint
> / CometBFT: "less than 1/3 of validators by voting power are
> Byzantine." Ethereum consensus-specs operates on "honest
> validators" with a formal definition rather than a percentage
> statement in prose.
>
> Three distinct rhetorical templates emerge:
>   (a) **Positive-condition prose** ("if honest nodes …") — common
>       in informal whitepapers.
>   (b) **Adversary-bound formula** ("at most ⌊n/3⌋ Byzantine …") —
>       common in academic / formal specs.
>   (c) **Defined-term style** ("an honest validator is one that …")
>       — common in executable / formal-method specs.
>
> The Foretias style guide will pick a primary style and
> (optionally) a secondary style for informal sections, then enforce
> it.

The cookbook covers the same shape of analysis for:
- Safety (no two honest nodes commit conflicting outputs)
- Liveness (some honest decision is eventually reached)
- Network synchrony (Δ-bounded delay, partial synchrony, asynchronous)
- Fault tolerance bounds (crash vs. Byzantine, threshold)
- Eventual consistency / convergence
- Capacity / throughput / latency claims
- Stability under load (backpressure, congestion control)
- Privacy / confidentiality
- Forward / post-compromise security
- Censorship resistance / liveness-under-adversary
- Trust roots and trust transitivity

### 5-C. Hardware-spec patterns absent from software specs
What do USB / Bluetooth / PCIe / TPM / I2C / NVMe do that Ethereum /
TLS / libp2p don't? Candidates the research will confirm or refute:
- Timing diagrams and electrical parameters
- Mandatory test laboratories and certification programs
- Vendor-ID registries
- Plug-fest / interop events
- Optional-feature negotiation handshakes (e.g., USB descriptors)
- Multi-vendor IP pool / patent pledges
- "Self-certify" vs. "third-party-certify" conformance distinction
- Profile registries managed by the standards body

For each pattern: which translate to "Foretias hardware service tier,"
which are SDO-overhead the project can skip.

### 5-D. Adoption correlates
Across the corpus, what spec-design traits correlate with high adoption?
Spec length, profile count, test-suite presence, reference
implementation count, patent policy, free vs. paywalled, existence of a
quickstart? Observational, not causal; confounds called out.

Sub-questions to answer in the same analysis:
- **Tone correlates** (per 1.5.K). Do warmer / more-inviting spec
  voices correlate with more independent implementations? Or are
  the dryest specs (TLS, NVMe) sometimes the most-implemented
  *because* they are dry?
- **Community-infrastructure correlates.** Does the visible
  presence of a CoC, contributor onboarding doc, or public
  discussion channel correlate with contributor diversity?
- **Machine-readability correlates.** Does shipping
  machine-extractable schemas / test-vector files / executable
  specs correlate with faster adoption in the recent (post-2020)
  cohort?
- **Fair-and-balanced correlates.** Does the presence of explicit
  limitations / failure-condition / "what we don't do" sections
  correlate with sustained trust over time? Or is omission the
  genre norm in highly-adopted infrastructure?

### 5-E. Failure-mode lessons
From counterexamples (4-F): what do failed or struggling protocols do
differently? Over-specification, under-specification, ambiguous
normativity, missing test vectors, fragmented profiles, IP problems,
opaque governance.

### 5-F. Foretias-specific style synthesis
Pull 5-A through 5-E into a single style guide: phrasing rules,
layering rules, normative conventions, terminology discipline, diagram
style, length budgets.

### 5-G. Foretias document-family recommendation
Concrete proposed file list for the eventual Foretias Protocol corpus,
each with audience and layering rules.

### 5-H. Primitive-layer vs application-layer spec patterns
**(Stack-positioning synthesis, per 1.5.J.)** Across the corpus,
classify each protocol by stack position (primitive / common-denominator
/ application / vertical) and identify which spec-writing patterns
correlate with each position. Hypotheses to test:
- Primitive-layer specs are shorter per normative document, narrower in
  audience, and more reference-disciplined toward lower layers.
- Common-denominator specs (libp2p, gRPC) ship as multi-document
  *families* with maturity tags, more than primitives do.
- Application-layer specs allow themselves more narrative and more
  end-user voice.
- Vertically-integrated specs (USB device classes, Bluetooth profiles)
  treat the lower layer as out-of-scope-by-reference and elaborate the
  domain.

Output: a one-page "spec voice by stack position" table that the
Foretias style guide (5-F) can quote directly when choosing voice for
each document in the recommended family (5-G).

---

## Part 6 — Plan (phases) — with sub-agent offload

For grunt work — bulk downloads, repetitive rubric fill-ins, quote
extraction — the plan delegates to sub-agents that work in parallel.
Each sub-agent receives a self-contained brief (target URL + activity
list + rubric template) and returns a structured artifact (downloaded
file inventory, filled rubric, quote list). The main agent reviews
quality and integrates. Sub-agents are *not* used for the synthesis
stages (Phases 3–5) — synthesis is judgment work, not grunt work.

### Phase 0 — Prior-art survey (arxiv and adjacent academic sources)

Before committing to a full corpus survey, a sub-agent searches for
existing meta-analyses that may already answer parts of this
project's research questions. Duplicating an existing published
survey wastes effort; citing and building on one strengthens the
output.

- **0.1** Sub-agent (own worktree, own branch) searches for prior
  work on:
  - Protocol specification design and patterns
  - Comparative analysis of standards organizations (IETF, IEEE,
    W3C, ISO, USB-IF, Bluetooth SIG, TCG, JEDEC, fund-of-funds)
  - Adoption dynamics of open standards
  - Empirical studies of spec quality, ambiguity, conformance
  - Documentation patterns in distributed-systems and
    cryptographic-protocol literature
  - Formal-method coverage of major protocols (TLS, QUIC, MLS,
    Signal, consensus protocols)
  - Existing surveys of P2P / blockchain / hardware protocols
- **0.2** Search sources (sub-agent retrieves full PDFs where
  available):
  - arxiv.org listings cs.CR (cryptography), cs.DC (distributed
    computing), cs.NI (networking), cs.SE (software engineering)
  - Google Scholar
  - ACM Digital Library, IEEE Xplore, USENIX (URL only when
    paywalled)
  - SIGCOMM, IMC, USENIX Security, S&P, NDSS conference
    proceedings indices
  - IETF working-group documents and IRTF research-group reports
- **0.3** Sub-agent commits one file per surveyed paper to
  `research/prior_art/<paper-slug>.md`, each with: full citation,
  link to PDF (or stored path if downloaded), abstract,
  10–20-line "what it covers / what it doesn't / supersedes-or-
  builds-on assessment." Progressive commits — one paper = one
  commit.
- **0.4** Sub-agent produces `research/PRIOR_ART_SURVEY.md`
  summarizing: total papers reviewed, papers worth deep-reading,
  any paper that materially supersedes parts of this project
  (in which case Phase 1+ scope contracts to "extend X" rather
  than "redo X").

**Phase 0 gate:** human review of the prior-art catalog before
Phase 1 proceeds. If a strong meta-analysis exists, this entire
spec/plan is amended to build on top of it.

### Phase 1 — Corpus confirmation and acquisition

- **1.1** Human review of Part 4 corpus list. Add / remove /
  annotate. Answer Q1–Q18 in Part 9.
- **1.2** Acquisition. One sub-agent per Part 4 sub-category
  (4.A, 4.A-bis, 4.B, 4.C, 4.D, 4.E, 4.F). Each sub-agent runs in
  its own git worktree on its own branch per Part 6.5
  (execution model). Each sub-agent:
  - Creates its category subdirectory.
  - Retrieves **full documents** (not excerpts) — `curl` / `wget`
    for PDFs and standalone HTML, `wget --mirror --no-parent
    --convert-links` for multi-page documentation sites,
    `git clone --depth 1` for spec repositories.
  - Writes one `INDEX_<protocol>.md` per acquired protocol with
    the exact `curl` / `wget` / `git clone` command used (so
    re-acquisition is byte-reproducible), source URL, acquired
    file paths, file sizes, and SHA-256 of each PDF.
  - **Commits progressively** — one commit per protocol
    acquired (download + its `INDEX_<protocol>.md` entry), not
    one final batch commit.
  - Records paywalled specs in a category `PAYWALLED.md` with
    the URL the human reviewer must visit manually.
  - Returns a 5-line completion report listing acquired count,
    paywalled count, total bytes, total commits.
- **1.3** Main agent merges per-category indices into a single
  top-level `INVENTORY.md` in its own worktree.

**Phase 1 gate:** human signs off on inventory before Phase 2 begins.

### Phase 2 — Per-document survey

**Study order within Phase 2** (sequencing only — the corpus, rubric,
and deliverables don't change; this is just the order sub-agents are
dispatched in).

- **Priority A — Multi-implementation systems first.** Specs with
  many independent, interoperable implementations across
  languages/hardware have *proven* their cross-implementation
  coordination — they are the strongest evidence for what works.
  Studying these first builds the strongest pattern library. This
  cohort: Ethereum (Geth/Nethermind/Besu/Erigon/Reth + Prysm/
  Lighthouse/Teku/Nimbus/Lodestar), Bitcoin (Core, btcd, libbitcoin,
  others), TLS 1.3 (OpenSSL/BoringSSL/GnuTLS/NSS/SChannel/rustls/…),
  QUIC (many), libp2p (go/rust/js/nim/jvm), IPFS
  (kubo/js-ipfs/iroh), Matrix (Synapse/Dendrite/Conduit/…),
  HTTP/1.1 / HTTP/3, DNS (BIND/Unbound/Knot/…), gRPC, Protobuf,
  CBOR, USB (countless vendor stacks), Bluetooth (BlueZ + many),
  TPM 2.0 (multi-vendor by design), FIDO2 (multi-vendor),
  WebAuthn (browsers), Noise, Signal, MLS, WireGuard, SSH
  (OpenSSH/libssh/Dropbear/paramiko/russh).

- **Priority B — Directly related to Foretias's problem space.**
  After Priority A, study everything in Foretias's adjacency:
  all timestamping ancestors (4.A-bis — RFC 3161, RFC 5816,
  OpenTimestamps, Roughtime, NTS, CT/RFC 9162, Sigstore),
  all blockchain/consensus (Tendermint/CometBFT, Ethereum
  consensus-specs, Ethereum execution-specs), all distributed
  storage and proof-of-storage (4.H — Filecoin, Chia, Arweave,
  Storj, Sia, GFS/HDFS/Lustre/Ceph/Tahoe-LAFS/Dynamo, Swarm),
  remaining P2P (SSB, Hypercore, ActivityPub).

- **Priority C — Low and wide; cover everything else.** All
  remaining corpus entries: internet infrastructure not yet covered
  (TCP/IP/BGP/SMTP/RTP/RFCs 2119/8174/2026/8729), remaining
  cryptographic protocols (OAuth 2.1, OpenID Connect, X3DH),
  remaining hardware specs (PCIe, NVMe, IEEE 802.11 / 802.3, SD,
  I2C, CAN, EMV, UEFI, ACPI, AMBA, JEDEC, HDMI, DisplayPort,
  Thunderbolt), governance docs (W3C Process, USB-IF ECN, TCG
  Public Review, IEEE-SA), and counterexamples (OpenPGP, XMPP,
  SOAP/WS-*, IPv6).

The priorities are **dispatch ordering**, not depth ordering. Each
subject is studied **thoroughly in a single pass** to minimize
revisits — a Priority C subject gets the same rubric depth as a
Priority A subject when its turn comes. The downloaded source
documents from Phase 1.2 remain in the worktree and are accessible
throughout Phases 2–5, so revisits are cheap when they do happen
(e.g., during synthesis); the goal of single-pass thoroughness is to
make revisits *unnecessary* most of the time.

- **2.1** Author the rubric template (Part 3) as a fillable markdown
  form, committed to `foretias/specs/research/RUBRIC_TEMPLATE.md`.
- **2.2** **Pilot.** Main agent fills the rubric for 3 carefully
  chosen exemplars: Bitcoin whitepaper (informal narrative), TLS 1.3
  RFC 8446 (formal prose), USB device-class spec (hardware profile).
  Human reviews these three to confirm the rubric is producing
  useful output before scaling.
- **2.3** **Scale via sub-agents.** Each sub-agent works in its own
  git worktree on its own branch per Part 6.5. The sub-agent's brief
  contains: assigned protocol(s), acquisition path(s), rubric
  template, Part 2 reading-checklist activities, and the Part 1.5
  Foretias compare-and-contrast roster. The sub-agent:
  - Reads the assigned protocol's full acquired document(s).
  - Fills one rubric file at `corpus_index/<protocol>.md`.
  - Appends 3–5 verbatim quotes to its own
    `corpus_index/cookbook_fragments/<protocol>.md` (per-source
    separation — never appending to a shared file from inside a
    worktree).
  - Writes a 5-line "notable surprises" note inside the same
    rubric file.
  - **Commits progressively** — one commit per completed rubric
    (one protocol per commit). Many sub-agents work in parallel
    without contention because each writes to its own files in its
    own worktree.
- **2.4** Main agent does light QA per returned worktree, requests
  re-runs where output is shallow, and merges completed
  worktree branches.
- **2.5** After all sub-agents have merged, the main agent
  consolidates the per-source `cookbook_fragments/*.md` into the
  running `PHRASING_COOKBOOK_DRAFT.md`. Consolidation preserves
  per-source attribution; quotes are never anonymized.
- **2.6** All filled rubrics land in
  `foretias/specs/research/corpus_index/`. The per-source
  separation rule (Part 6.5.4) holds throughout.

### Phase 3 — Cross-cutting synthesis (no sub-agents)

- **3.1** Median-shape analysis (5-A).
- **3.2** Phrasing cookbook finalization (5-B).
- **3.3** Hardware-pattern analysis (5-C).
- **3.4** Adoption-correlate analysis (5-D).
- **3.5** Failure-mode analysis (5-E).
- **3.6** Draft findings report at
  `foretias/specs/research/FINDINGS.md`.

Sub-agents may be used in this phase only for narrow re-readings ("go
back to corpus entry X and find quotes about Y") — not for the
synthesis itself.

### Phase 4 — Foretias-specific recommendations

- **4.1** Style guide draft (Deliverable 6).
- **4.2** Document-family recommendation draft (Deliverable 5).
- **4.3** Hardware service-tier playbook (Deliverable 4).
- **4.4** Phrasing cookbook in final form (Deliverable 3).

### Phase 5 — Templates

- **5.1** Empty skeleton for each document in the recommended family.
- **5.2** First short fill-in for one chosen document, to test the
  template against real Foretias content. Proposed test subject: re-cast
  the existing `CALENDAR_REPLICATION_SPEC.md` content through the new
  template and compare.

### Phase 6 — Human review checkpoint (hard stop)

User reviews Deliverables 1–7 before any commitment to authoring the
Foretias Protocol corpus.

### Phase 7 — (Out of scope here.) Authoring the Foretias Protocol corpus

Tracked as a separate future Major informed by this project's output.

---

## Part 6.5 — Execution model: parallel agents, worktrees, progressive commits

This project is intentionally parallel. Multiple agents work
simultaneously, each on its own slice of the corpus. The execution
model below ensures their work composes cleanly, is recoverable at any
point, and never silently overwrites another agent's output.

### 6.5.1 — One agent per git worktree, one branch per agent

The coordinator (human or coordinator agent) creates a dedicated git
worktree for each parallel sub-agent **before** assigning work. Each
worktree:

- Lives at a distinct filesystem path under a sibling directory of
  the main repo (e.g., `../foretias-research-prior-art/`,
  `../foretias-research-corpus-4a/`,
  `../foretias-research-corpus-4d/`).
- Is checked out on a distinct branch with the convention
  `research/<phase>-<slice>` (e.g., `research/phase0-prior-art`,
  `research/phase1-corpus-4a-blockchain`,
  `research/phase2-rubric-bitcoin-whitepaper`).
- Is the agent's *only* working directory for the duration of its
  assignment.

The agent's brief explicitly includes: worktree absolute path,
branch name, target list, deliverable file paths, commit-cadence
rules, and the Part 1.5 Foretias roster.

### 6.5.2 — Stay-in-worktree rule (no-cd rule)

The agent does **not** `cd` elsewhere during its assignment. All file
operations use absolute paths within the assigned worktree. The
agent does not read or write outside its worktree, except:

- Reading source URLs over the network (curl / wget / git clone of
  remote sources is by definition not a local filesystem move).
- Reading the parent project's reference documents (this spec,
  `wdocs/`, `foretias/specs/`) by absolute path, **read-only**.

Rationale: parallel agents on different worktrees interfere if any
agent assumes a shared working directory, runs git commands without
explicit `-C` / `--git-dir`, or modifies shared files outside its
branch. The no-cd rule keeps each agent hermetic.

### 6.5.3 — Progressive commits

The agent commits as each artifact completes, not in one final
batch:

- **Phase 0:** one commit per surveyed paper.
- **Phase 1.2:** one commit per protocol acquired (downloaded
  files + that protocol's `INDEX_<protocol>.md` entry).
- **Phase 2.3:** one commit per rubric completed (one protocol per
  commit).
- **Phase 3–4:** one commit per synthesis section drafted.
- **Phase 5:** one commit per template skeleton produced.

Commit-message convention (matches the project's existing rule from
`webhash/CLAUDE.md`):

```
Major: Protocol-Authoring Research, Phase <N>.<m> — <activity>
agent: <agent-id>; worktree: <worktree-name>; branch: <branch>
<tool/model line per CLAUDE.md global rule>
```

Rationale: progressive commits make partial work recoverable, make
review chunked rather than monolithic, and let the coordinator merge
completed branches without waiting for the slowest worker.

### 6.5.4 — Per-source separation in documentation

Reading results from one protocol are **never** combined with another's
in the same output file. Each protocol gets:

- Its own acquisition path under
  `protocol_references/<category>/<protocol>/`.
- Its own `INDEX_<protocol>.md` recording the acquisition command and
  artifacts.
- Its own rubric file at `corpus_index/<protocol>.md`.
- Its own per-source cookbook fragment at
  `corpus_index/cookbook_fragments/<protocol>.md`.

Synthesis stages (Phase 3+) cross-reference multiple protocols by
file path, not by inlining content. The per-source files are the
source of truth; synthesis is derived. This guarantees that two
agents writing about two protocols simultaneously cannot create a
merge conflict in the same file.

### 6.5.5 — Full-document acquisition

The agent prefers full-document retrieval over selective excerpts:

- PDFs: `curl -sLo <file> <url>`.
- Standalone HTML: `curl -sL <url> -o <file>.html`.
- Multi-page documentation sites: `wget --mirror --no-parent
  --convert-links --page-requisites --html-extension <url>` (or
  equivalent), producing a fully self-contained local mirror.
- Spec repositories: `git clone --depth 1 <url> <dest>`.
- IETF RFCs: prefer the canonical `https://www.rfc-editor.org/rfc/<id>.txt`
  for archival stability; mirror the HTML form too if needed.
- arxiv papers: `curl -sLo <id>.pdf https://arxiv.org/pdf/<id>.pdf`.

The acquisition command is recorded verbatim in the per-source
`INDEX_<protocol>.md` so any later agent can re-acquire byte-for-byte.

Storage is cheap. Re-fetching mid-survey breaks reproducibility and
risks the upstream having silently changed. Acquire once, fully,
locally; cite the local copy from then on.

### 6.5.6 — Coordinator responsibilities

A coordinator agent (or human) is responsible for:

- Creating worktrees and branches before dispatching briefs.
- Dispatching briefs with all context the sub-agent needs (no
  assumption that the sub-agent will read the surrounding repo).
- Reviewing completed branches and merging them back to the
  project's working branch.
- Resolving any cross-worktree conflicts (rare given the
  per-source-file convention).
- Running the synthesis phases (3–5) in its own dedicated worktree
  rather than in any of the per-corpus-slice worktrees.

### 6.5.7 — Sub-agent brief template

The coordinator's brief to each sub-agent contains, at minimum:

```
Worktree path:    <absolute path>
Branch name:      <research/...>
Phase / stage:    <e.g., 2.3>
Assigned target:  <protocol name(s) and acquisition path(s)>
Deliverables:     <exact file paths the sub-agent must create>
Commit cadence:   <one commit per <unit>>
Reference docs:   <this spec, Part 1.5 roster, Part 2 checklist,
                  Part 3 rubric template>
Constraints:      Stay-in-worktree (Part 6.5.2); progressive
                  commits (Part 6.5.3); per-source separation
                  (Part 6.5.4); full-document acquisition (Part 6.5.5)
Reporting:        Short completion report on finish — counts, paths,
                  any blockers
```

The brief is **self-contained** — the sub-agent does not need to
reconstruct context from chat history.

---

## Part 7 — Directory layout (specification only — not created yet)

When Phase 1.2 runs, the following layout will be created under
`foretias/specs/research/`. *Nothing in this layout exists yet.* The
spec lists it here so reviewers can see and amend it before any
filesystem changes.

```
foretias/specs/research/
├── RUBRIC_TEMPLATE.md             # Phase 2.1
├── INVENTORY.md                   # Phase 1.3
├── PHRASING_COOKBOOK_DRAFT.md     # Phase 2.5 (running)
├── FINDINGS.md                    # Phase 3.6
├── PHRASING_COOKBOOK.md           # Phase 4.4 (final)
├── HARDWARE_TIER_PLAYBOOK.md      # Phase 4.3
├── PROTOCOL_FAMILY_RECOMMENDATION.md  # Phase 4.2
├── PROTOCOL_STYLE_GUIDE.md        # Phase 4.1
│
├── protocol_references/           # Phase 1.2 — acquired corpus
│   ├── 4a_blockchain_p2p/
│   │   ├── INDEX.md
│   │   ├── PAYWALLED.md
│   │   └── <per-protocol files / clones>
│   ├── 4b_crypto_transport/
│   ├── 4c_internet_infrastructure/
│   ├── 4d_hardware/
│   ├── 4e_process_governance/
│   └── 4f_counterexamples/
│
├── corpus_index/                  # Phase 2.6 — filled rubrics
│   └── <one file per corpus entry>
│
└── templates/                     # Phase 5 — proposed Foretias
    └── <one skeleton per recommended document>
```

The `foretias/specs/research/protocol_references/` and
`foretias/specs/research/corpus_index/` directories are created by
sub-agents in their respective phases. Nothing is committed to
git in this spec/plan document beyond this file.

---

## Part 8 — Deliverables (recap)

1. **Annotated corpus index** — one filled rubric per protocol
   (Part 3 template). `corpus_index/<protocol>.md`.
2. **Cross-cutting findings report** — answers 5-A through 5-E.
   `FINDINGS.md`.
3. **Phrasing cookbook for large guarantees** — answers 5-B in full.
   `PHRASING_COOKBOOK.md`.
4. **Hardware ↔ software service-tier playbook** — answers 5-C.
   `HARDWARE_TIER_PLAYBOOK.md`.
5. **Foretias Protocol document family — recommended layout** —
   answers 5-G. `PROTOCOL_FAMILY_RECOMMENDATION.md`.
6. **Foretias Protocol style guide** — answers 5-F.
   `PROTOCOL_STYLE_GUIDE.md`.
7. **Templates** — empty skeleton per recommended document.
   `templates/`.

---

## Part 9 — Open questions for human reviewer

Before Phase 1 begins, please confirm or amend:

- **Q1. Corpus scope.** Is the Part 4 list right-sized? Add / remove
  what? In particular: 3GPP / 5G (4-G) is the most-adopted
  hardware/software-layered spec family on earth but is enormous.
  Include? Subset? (@Agent lgtm)
- **Q2. Hardware-tier depth.** Light (study patterns only) or deep
  (also study electrical / timing / certification-program conventions)?
- **Q3. Paywalled specs.** Skip them, summarize from open sources, or
  ask the human reviewer to acquire? (@Agent summarize from open sources,
  but note paywall. Most of what we're interested in are not behind paywall)
- **Q4. Effort budget cap.** A 40–55 entry corpus at 1 hr / entry plus
  synthesis is roughly 60–90 hours of equivalent work. Cap? Or
  open-ended? (@Agent no cap, only cap is amount of work done before a new
  summary file is produced. We should be producing/updating document at
  least once every 15 minutes)
- **Q5. Code-reading depth.** Part 2.B suggests "1 in 5 protocols" gets
  a reference-implementation read-through. (@agent at least read examples,
  read API declarations. major API endpoints, and core protocols code)
- **Q6. Sub-agent budget.** Sub-agent offload (Part 6) can dramatically
  parallelize Phases 1–2 but consumes API quota. Cap on concurrent
  sub-agents? Total spend? (@Agent no cap. just has to write to disk
  summary or progress every 15 minutes or so)
- **Q7. Audience for the eventual Foretias Protocol.** Implementer-only,
  or also regulator / integrator / end-user?(@Agent, audiance of
  the protocol document are mainly human or agent coders who will use
  foretias system. Some scientists who wants to research it, plus curious
  users who wants to know more)
- **Q8. Adoption-target shape.** Many independent implementations
  (Ethereum)? Wide end-user install base (Bluetooth)? Both?
  (@Agents, ideally, we'd like this rust implementation to become useful
   enough and secure enough to demonstrate that the system is useful.
   Others may leverage the library or reimplement it with custom security
   configurations or hardware. Later hardware implementations as well.)
- **Q9. Style preference.** Lean toward IETF-RFC formal prose,
  Yellow-Paper math, executable-spec, narrative whitepaper, or
  deliberate pluralism across documents?
  (@Agent, Description, diagrams, flow chart, swimming lane. rust code snippets
   whatever it takes to clearly explain everything--except for animations or videos)
- **Q10. Versioning model preference.** Single living spec, versioned
  editions, or numbered improvement proposals?
  (@Agent, yes versioned)
- **Q11. Big-guarantee phrasing preference.** Of the three templates
  illustrated in 5-B (positive-condition prose / adversary-bound formula
  / defined-term style), is there an early preference for Foretias, or
  should research stay agnostic? (@Agent, this is the preference unless
  we find and understand other protocols doing it differently)
- **Q12. Scope of "Foretias Protocol" itself.** Does it include the
  custom-plugin crypto layer, or stop at the host-software boundary?
  (@Agent, this can be a qualitative "secure", but more colorful and
   specific, clearly meaning "as secure as you make it".)
- **Q13. Stack-position weighting.** Given Foretias's
  common-denominator / primitive positioning (1.5.J), should the
  corpus over-weight primitive-layer protocols (4.A-bis, 4.B, parts
  of 4.D like I2C / TPM-as-primitive) and under-weight
  application-layer ones (some of 4.A, parts of 4.D)? The current
  list is balanced; the human reviewer may prefer skew.(@Agent, foretias
  currently is a library and a server, its function is distributed
  p2p-based time stamping, this is a portion of many cryptocurrency systems)
- **Q14. Tone target for the Foretias Protocol corpus** (per 1.5.K).
  Pick an early reference point so Phase 4 isn't wide-open: closest
  to TLS RFCs (dry / formal), libp2p specs (community-led /
  conversational), Rust RFCs (warm / "we"), Bitcoin Core
  (terse / minimal warmth), gRPC docs (integrator-friendly), or
  pluralism across documents? Or stay agnostic until the corpus
  survey is in? (@agent, closest to Ethereum)
- **Q15. AI-coding-agent readability as a first-class requirement**
  (per 1.5.K, machine-readability sub-section). Treat as a hard
  requirement (every normative document ships with machine-extractable
  schema and test-vector files), a soft target (best-effort), or
  out of scope?
  (@Agent, this is a learning experience. let's see if we find any agents.md
   or other documentation directed towards agents)
- **Q16. Whitepaper vs. protocol-corpus voice separation.** Confirm
  the assumption that the existing `whitepaper.md` voice serves
  non-developer stakeholders and the protocol corpus is allowed to
  be developer-first without trying to re-please non-developers in
  every document? (@Agents protocol is for develoeprs and researchers)
- **Q17. Code of Conduct / community-infrastructure scope.**
  Should this research project also recommend CoC posture,
  contributor onboarding shape, and public-discussion-channel
  choice — or leave those to a separate governance Major?
  (@Agents code of conduct is more to reinforce alignment of intentions
   and practices--towards reliable and trustworthy time stampping)
- **Q18. Fair-and-balanced posture target.** The whitepaper has
  already chosen explicit honesty about failure modes. Should the
  protocol corpus inherit that posture as a style-guide rule, or
  is each document allowed to follow its own genre's convention
  (terse RFCs without "Security Considerations" if the genre
  permits)?
  (@Agents of course. But we are more focused on solutions and
    facilities to achieve desired goals. So even if we do talk
    about failure modes, they are expressing opportunity or outter
    boundaries for when system will fail)

---

## Part 10 — Scope clarification (in / out)

**In scope** (clarified after first review):
- Carrying the Part 1.5 Foretias roster *in mind* while reading every
  corpus entry; recording compare-and-contrast notes per rubric.
- Identifying adoption candidates and counter-patterns specifically
  for Foretias.
- Drafting recommendations and templates that bake in Foretias's
  known shape (mission, crypto stack, P2P stack, identity model,
  service tiers, non-goals).

**Out of scope**:
- Re-deriving any cited protocol's technical content.
- Editorial criticism of cited protocols (note styles, don't grade).
- Authoring the Foretias Protocol itself (separate future Major).
  Recommendations and templates are produced here; load-bearing
  Foretias Protocol prose is not.
- Building any Foretias features (no production code touched).
- Designing crypto or hardware (study only how others have *written
  about* their crypto and hardware).
- Translating cited protocols into other languages.
- Bibliographic completeness — the corpus is curated for lesson
  density, not exhaustiveness.

---

## Part 11 — Tracking

- This document:
  `foretias/specs/PROTOCOL_AUTHORING_RESEARCH_SPEC_PLAN.md`.
- Phase 1.2 will create the corpus directory:
  `foretias/specs/research/protocol_references/`. **Not yet created.**
- Phase 2.6 will create the rubric directory:
  `foretias/specs/research/corpus_index/`. **Not yet created.**
- Phases 3–5 deliverables will land directly in
  `foretias/specs/research/`. **Not yet created.**
