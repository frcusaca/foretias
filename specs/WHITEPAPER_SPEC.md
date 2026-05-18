# Spec/Plan: Foretias Public Whitepaper

- [x] backburnered

**Scope:** Major — public-facing document
**Status:** Draft in progress — research phase not yet complete

---

## Part 1 — Plan and Research Todos

The goal is a polished, publicly releasable whitepaper that introduces Foretias
to a general audience: technically curious readers who are not necessarily
cryptographers, but who are familiar with Bitcoin, Ethereum, or online banking
at a conceptual level. Tone is formal but accessible — serious enough to be
taken seriously, plain enough that a non-engineer can follow it. This is an
extended Executive Summary, an opening pitch in a sales meeting.

The document does have one tighter section to market to engineers Here we will
use big words and big numbers, describe the library, how it is easily available
in every language binding. Library is coded with safety and security in mind, using Rust.
Mention prominant libraries,.. cryptographic implementation from libsodium, etc.
We use the same libp2p library that ethereum uses to establish consensus. Foretias
library is configured to use Post Quantum Cryptography that are safe against
attacks by quantum computers.... The history is preserved by conseus through history.

### Terminology decision (locked)

- Each time interval in a Time Being's calendar is called a **chronon**.
- The append-only chain of cryptographically linked chronons is called the
  **Chrononchain**. This term is used in the whitepaper as the Foretias
  analogue of "blockchain" — readers already familiar with Bitcoin or Ethereum
  will immediately understand the structural parallel.

### Writing guidelines

- Use "Chrononchain" when the chain structure is the point.
- Use "calendar" when describing what a Time Being stores and serves.
- Never say "blockchain" in reference to Foretias itself; say "Chrononchain."
  "Blockchain" may be used when describing Bitcoin or Ethereum for comparison.
- Foretias has exactly **two services**: Stamp and Verify. Do not describe
  future features (cross-attestation, probity, epoch consensus) as present
  capabilities.
- All comparisons to other systems must be accurate. Verify claims during
  research before writing.

---

### Research Todos

For each project below: fetch the official whitepaper or homepage description,
extract key phrases used to explain the project to non-technical readers, note
how they describe decentralization and trust, and record any memorable
one-liners. This research informs the "Familiar Ground" section of the
whitepaper — so Foretias can be positioned accurately relative to things
readers already know.

#### Blockchain and Cryptocurrency (distributed ledgers, P2P value transfer)

- [ ] **Bitcoin (BTC)**
  - Satoshi Nakamoto original whitepaper (2008) — how it describes the
    double-spend problem and the trust model
  - bitcoin.org plain-English audience pages
  - Key concept to extract: how "chain of blocks" and "proof of work" are
    explained without jargon
  - The white paper that we want to mimic is probably: https://bitcoin.org/en/how-it-works
    Probably a little bit more text is okay.

- [ ] **Ethereum (ETH)**
  - ethereum.org/en/whitepaper — overview and intro sections
  - ethereum.org/en/what-is-ethereum — audience-facing description
  - Key concept to extract: how "smart contracts" and "decentralized
    applications" are explained; how they position Ethereum relative to Bitcoin
  - This white paper is more extensive: https://ethereum.org/whitepaper/ This is NOT 
    the level of detail we want right now. We want to introduce a human being to
    a completely new capability suddenly available to them on the computers.

- [ ] **Solana (SOL)**
  - solana.com — main site and whitepaper intro
  - Key concept to extract: how "Proof of History" is described — this is
    directly relevant because Solana also addresses ordering of events in time

- [ ] **XRP Ledger (XRP)**
  - xrpl.org — overview
  - Key concept to extract: how "consensus without mining" is explained

- [ ] **Cardano (ADA)**
  - cardano.org — overview and philosophy pages
  - Key concept to extract: how "peer-reviewed" and "formal methods" are
    described for a general audience; their approach to trust and rigor

- [ ] **Monero (XMR)**
  - getmonero.org — overview
  - Key concept to extract: how privacy properties are explained without
    cryptographic detail

- [ ] **Polkadot (DOT)**
  - polkadot.network — overview
  - Key concept to extract: how "interoperability" and "shared security"
    are explained

#### Decentralized Storage and Web3 (P2P data networks)

- [ ] **IPFS / Filecoin**
  - docs.ipfs.tech — "what is IPFS" pages
  - filecoin.io — overview
  - Key concept to extract: how "content addressing" replaces "location
    addressing"; how permanence and availability are described
  - IPFS is a software library, which directly maps to how engineers approaches
    Foretis system.

- [ ] **Arweave (AR)**
  - arweave.org — overview
  - Key concept to extract: how "permanent storage" is described and
    motivated; the "permaweb" framing

- [ ] **Bittensor (TAO)**
  - bittensor.com — overview
  - Key concept to extract: how decentralized AI and incentive structures
    are described

- [ ] **Akash Network (AKT)**
  - akash.network — overview
  - Key concept to extract: how "decentralized cloud" is positioned against
    AWS/Azure

#### P2P Communication and File Sharing (foundational P2P protocols)

- [ ] **BitTorrent**
  - bittorrent.org/beps/bep_0003.html — original protocol description
  - bittorrent.com — audience-facing pages
  - Key concept to extract: how "swarm" and "seeder/leecher" are explained;
    how availability without a central server is described

- [ ] **Matrix**
  - matrix.org — overview and "What is Matrix?" pages
  - Key concept to extract: how "federated" vs "centralized" communication
    is explained; how rooms and servers are described without jargon

- [ ] **Syncthing**
  - syncthing.net — overview
  - Key concept to extract: how P2P sync without a cloud server is motivated
    and described for everyday users

#### P2P Marketplaces

- [ ] **OpenBazaar**
  - Historical documentation (project now inactive, use archived pages)
  - Key concept to extract: how "no listing fees, no intermediary" was
    positioned; the trust-without-platform model

---

### Research Output Format

For each project, record findings as a short entry:

```
## [Project Name]
Source: [URL or document]
One-liner they use: "..."
How they describe decentralization: ...
How they describe trust: ...
Relevant vocabulary: [list of key terms they use]
Relevance to Foretias whitepaper: [what we can borrow, adapt, or contrast]
```

Collect findings in `specs/WHITEPAPER_RESEARCH.md` (to be created during
research phase).

---

### Whitepaper Sections (outline — to be written after research)

1. **Abstract** — two sentences: the problem, the solution
2. **The Problem** — digital timestamps are forgeable; central authorities are
   single points of failure
3. **The Foretias Approach** — key destruction makes backdating structurally
   impossible; the Chrononchain
4. **Familiar Ground** — what Foretias shares with Bitcoin (chain structure),
   Ethereum/BitTorrent (P2P network), banking (cryptographic algorithms).
   Informed by research above.
5. **The Two Services** — Stamp and Verify, explained plainly
6. **Why It Can Be Trusted** — no organization to compromise, no clock to
   manipulate, open and auditable
7. **What It Is For** — use cases
8. **What It Does Not Do** — honest scope statement
9. **Summary**

---

## Part 2 — Current Draft

The draft below was written before the research phase. It will be revised once
research findings are incorporated. Sections marked with a note may change
significantly based on research.

*(Note: Section 3 "Familiar Ground" will be the most affected by research — the
specific comparisons and vocabulary used there should reflect how those projects
actually describe themselves, not how we assume they do.)*

---

# Foretias: A Trustworthy Record of When Things Happened

**Free and Open-source Resilient Time Integrity Attestation Service**

*Version 0.1 — 2026*

---

## Abstract

Foretias is an open-source timestamping service with exactly two operations:
**Stamp** and **Verify**. It makes it structurally impossible to claim that
something happened earlier than it actually did. Unlike traditional timestamp
services, Foretias does not rely on a central authority being honest or
available. Its guarantee comes from a cryptographic property — the deliberate
destruction of private keys — using the same algorithms that underpin modern
banking, government communications, and public blockchain networks. This
document explains what Foretias does and why it can be trusted.

---

## 1. The Problem: Digital Timestamps Are Easy to Fake

When a document, transaction, or event is recorded digitally, it is natural to
want to know *when* it occurred. In most systems today, a timestamp is simply a
claim: someone signed a piece of data and wrote a date beside it. The difficulty
is that this claim is easy to forge.

Anyone who controls a computer can set its clock to any time they wish. Anyone
who holds a cryptographic signing key can produce a signature that appears to be
from a year ago. Neither the clock nor the signature alone can prove that a
moment in time was genuine. Organizations have attempted to address this with
notary services and trusted third-party timestamp authorities, but each of these
solutions replaces the forgery problem with a different one: a single
organization that must be trusted, and a single target that can be compromised,
pressured, or simply taken offline.

---

## 2. The Foretias Approach

Foretias draws its core insight from a simple observation: **a private key that
has been destroyed cannot be used again.**

Every Foretias node — called a *Time Being* — generates a fresh cryptographic
keypair for each interval of time it operates through. Each such interval is
called a **chronon**. When one chronon ends and the next begins, the Time Being
destroys the old private key permanently. Once destroyed, that key is gone. No
backup exists. No one holds a copy.

Each chronon's record is cryptographically linked to the one before it, forming
an append-only **Chrononchain** — a tamper-evident log of every moment in the
Time Being's life. Anyone can inspect this chain and verify that no entry was
inserted, altered, or removed after the fact, because doing so would require
reconstructing signatures with keys that no longer exist.

This is the Foretias guarantee:

> A Foretias timestamp proves that content was presented to the network no
> earlier than the moment it claims, because the key that produced the
> signature has since been destroyed and cannot be recreated.

---

## 3. Familiar Ground: What Foretias Has in Common with Systems You Trust

*(This section will be revised after research phase.)*

Foretias did not invent its building blocks. It assembles well-understood,
widely deployed technology in a new configuration.

### Cryptographic signatures — the same standard as your bank

When you visit a secure website or authorize a bank transfer, your connection is
protected by public-key cryptography: a private key signs data, and anyone with
the corresponding public key can verify the signature. Foretias uses the same
class of algorithms — operating under the same mathematical principles that
governments and financial institutions worldwide have standardized for protecting
sensitive data.

### A chain of records — the same structure as Bitcoin's blockchain

Bitcoin maintains its integrity through a blockchain: each new block of
transactions cryptographically references the one before it. Altering any past
record would require recomputing every subsequent block — a task that grows
prohibitively expensive as the chain grows. Foretias applies the same structural
principle to time itself. The Chrononchain links each interval to its
predecessor with a cryptographic handshake. Inserting or altering any historical
chronon would require reconstructing every subsequent signature — with keys that
no longer exist.

### A P2P network — the same model as Ethereum and BitTorrent

Rather than routing all trust through a central server, Foretias operates as a
P2P network. Participants discover each other using Kademlia, the
distributed hash table algorithm deployed in the Ethereum network, the
InterPlanetary File System (IPFS), and BitTorrent. No single node controls the
network, and no single node's failure can disrupt it.

---

## 4. The Two Services

Foretias provides exactly two operations.

### Stamp

A user or application submits a piece of content to a Foretias Time Being. The
Time Being signs the content with the current chronon's private key and returns
a compact record called a *Foretis*. The Foretis contains the content's
fingerprint, the chronon number, and a cryptographic signature. It is small
enough to attach to any document, email, transaction log, or database entry.

From this moment forward, the existence of the Foretis is evidence that the
content was known at the time of stamping. The Time Being does not need to
understand the content — only to witness it.

### Verify

To verify a Foretis, any party — including one that was not involved in the
original stamping — presents the content and the Foretis to a Foretias node.
The node retrieves the corresponding entry from the Time Being's Chrononchain
and confirms that the signature in the Foretis was made by the key on record for
that chronon.

If the signature matches: the content was genuinely presented at that moment.
If it does not match, or if the chain entry does not exist: verification fails.
The check requires no contact with the original submitter, no trust in any
clock, and no special authority granted to the verifier.

Verification can be performed against any node that holds the relevant
Chrononchain — including nodes discovered automatically across the
P2P network.

---

## 5. Why Foretias Is Trustworthy

### No organization to compromise

There is no Foretias company holding a master key or managing a central
database. The network is a collection of independent nodes. Compromising the
network's integrity would require simultaneously compromising a large number of
independent participants — each holding short-lived keys that expire and are
destroyed on their own schedule.

### No clock to manipulate

Because Foretias's guarantee is based on cryptographic key destruction rather
than wall-clock time, an attacker who can manipulate a system clock gains
nothing. What matters is the order and structure of signatures in the
Chrononchain, not what any clock claims the time was.

### Open and auditable

Foretias is fully open source. The algorithms it uses are publicly documented
and independently implemented in security-critical infrastructure worldwide.
There are no proprietary components and no trust assumptions that cannot be
independently verified. Any competent cryptographer or security researcher can
read the specification and the source code and confirm that the system behaves
as described.

---

## 6. What Foretias Is Designed For

Foretias is general-purpose timestamping infrastructure suited to any situation
where the order and timing of events must be provable and tamper-resistant.

**Document and contract integrity.** Proving that a contract, patent application,
or policy document existed in its current form before a particular date, without
relying on a notary or third party to maintain the record.

**Audit trails for regulated industries.** Financial, healthcare, and legal
organizations can use Foretias to produce independently verifiable evidence that
logs were not altered after the fact.

**Software supply chain.** Build artifacts and release signatures can be
timestamped at the moment of production, allowing downstream users to verify
that a package existed before any known vulnerability disclosure.

**Inter-organizational trust.** When two organizations need to agree on the
ordering of events — whose request came first, when an obligation was fulfilled
— Foretias provides a shared, neutral record that neither party controls.

**Long-term archival verification.** Records timestamped today remain verifiable
years from now, because the Chrononchain and public keys are openly stored and
verification requires no contact with the original signer.

---

## 7. What Foretias Does Not Do

Foretias timestamps prove *when content was presented to the network*. Foretias
does not certify that the content is true, legal, or legitimate — only that it
existed in a specific form at a specific moment. It does not replace a legal
notary where one is required by law.

---

## 8. Summary

Foretias offers something that has been surprisingly difficult to achieve in
digital systems: a trustworthy, independently verifiable record of when things
happened, built on no trust assumption beyond the mathematics of the
cryptography itself.

The technology underneath Foretias is the same technology securing the world's
financial transactions, cryptocurrency networks, and government communications.
What Foretias contributes is a focused application of these tools to a specific
and valuable problem — temporal proof — delivered as two simple operations that
any application can call.

Time, once passed, cannot be altered. Foretias brings that same property to
the digital record.

---

*Foretias is free and open-source software. Specifications, source code, and
documentation are published openly for independent review.*
