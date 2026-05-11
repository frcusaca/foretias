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
keypair for each interval of time it operates through. When that interval ends
and the next one begins, the Time Being destroys the old private key
permanently. Once destroyed, that key is gone. No backup exists. No one holds
a copy.

This means that if you possess a valid signature produced by a particular Time
Being at a particular moment, that signature could only have been produced while
the Time Being was alive during that moment — because the key that made it no
longer exists. Backdating is not merely policy-forbidden; it is technically
impossible.

This is the Foretias guarantee:

> A Foretias timestamp proves that content was presented to the network no
> earlier than the moment it claims, because the key that produced the
> signature has since been destroyed and cannot be recreated.

---

## 3. Familiar Ground: What Foretias Has in Common with Systems You Trust

Foretias did not invent its building blocks. It assembles well-understood,
widely deployed technology in a new configuration.

### Cryptographic signatures — the same standard as your bank

When you visit a secure website or authorize a bank transfer, your connection is
protected by public-key cryptography: a private key signs data, and anyone with
the corresponding public key can verify the signature. Foretias uses the same
class of algorithms — operating under the same mathematical principles that
governments and financial institutions worldwide have standardized for protecting
sensitive data.

### A chain of records — the same structure as Bitcoin

Bitcoin maintains its integrity through a chain in which each new block of
transactions cryptographically references the one before it. Altering any past
record would require recomputing every subsequent block — a task that becomes
prohibitively expensive as the chain grows. Foretias uses the same structural
principle. Each interval in a Time Being's calendar cryptographically references
its predecessor. Inserting or altering a historical record would require
reconstructing every subsequent signature — with keys that no longer exist.

### A peer-to-peer network — the same model as Ethereum

Rather than routing all trust through a central server, Foretias operates as a
peer-to-peer network. Participants discover each other using Kademlia, the
distributed hash table algorithm deployed in the Ethereum network, the
InterPlanetary File System (IPFS), and BitTorrent. No single node controls the
network, and no single node's failure can disrupt it.

---

## 4. The Two Services

Foretias provides exactly two operations.

### Stamp

A user or application submits a piece of content to a Foretias Time Being. The
Time Being signs the content with its current private key and returns a compact
record called a *Foretis*. The Foretis contains the content's fingerprint, a
number identifying the moment within the Time Being's calendar, and a
cryptographic signature. It is small enough to attach to any document, email,
transaction log, or database entry.

From this moment forward, the existence of the Foretis is evidence that the
content was known at the time of stamping. The Time Being did not need to
understand the content — only to witness it.

### Verify

To verify a Foretis, any party — including one that was not involved in the
original stamping — presents the content and the Foretis to a Foretias node.
The node retrieves the corresponding entry from the Time Being's calendar and
confirms that the signature in the Foretis was made by the key on record for
that moment.

If the signature matches: the content was genuinely presented at that moment.
If it does not match, or if the calendar entry does not exist: verification
fails. The check requires no contact with the original submitter, no trust in
any clock, and no special authority granted to the verifier.

Verification can be performed against any node that holds the relevant
calendar — including nodes on the other side of the network, discovered
automatically via the peer-to-peer layer.

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
nothing. What matters is the order and structure of signatures in the calendar
chain, not what any clock claims the time was.

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
years from now, because the calendar chain and public keys are openly stored and
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
What Foretias contributes is a focused application of these tools to the
specific and valuable problem of temporal proof — delivered as two simple
operations that any application can call.

Time, once passed, cannot be altered. Foretias brings that same property to
the digital record.

---

*Foretias is free and open-source software. Specifications, source code, and
documentation are published openly for independent review.*
