# clerkb

Clerkb implements [Proof of Authority](https://en.wikipedia.org/wiki/Proof_of_authority) mechanism on [Nervos CKB](https://www.nervos.org/) for layer 2 solutions. It is designed as 2 components working together:

* 2 smart contracts used as lock scripts on CKB to validate logic on chain.
* A TypeScript based module for integrating PoA into your generator code.

The name comes from a combination of `clerk`, and CKB.

## Design

A `PoASetup` construct specifies the behavior of clerkb:

```typescript
export interface PoASetup {
  identity_size: number;
  round_interval_uses_seconds: boolean;
  identities: Array<HexString>;
  aggregator_change_threshold: number;
  round_intervals: number;
  subblocks_per_round: number;
}
```

One is free to specify the configurations here:

* The number of aggregators is determined by `identities` array. Clerkb is designed to support at most 255 aggregators.
* Each item in `identities` array contains the identity for one unique aggregator. The identity is represented as lock script hashes, or prefix of lock script hashes. Clerkb leverages the same technique as owner locks in [sUDT](https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0025-simple-udt/0025-simple-udt.md): an aggregator can unlock a cell governed by the PoA lock, as long as current transaction has an input cell, whose lock script hash is identical to the aggregator identity specified in `identitites` array.
* Each aggregator can issue L2 blocks in its own designated `round`. All the aggregators take turns having their own rounds. This is denoted by the order in `identitites` array. When the last aggregator in `identitites` array expires its round, the first aggregator in the array starts its round again.
* A round is capped in 2 ways:
    + `subblocks_per_round` determines how many layer 2 blocks can be issued per round
    + `round_intervals` determines the interval length of a round. Based on the value of `round_interval_uses_seconds`, the interval can either be expressed using seconds, or layer 1 blocks.
* The PoA setup can also be upgraded dynamically on chain. At least agreements(expressed via owner lock technique) from `aggregator_change_threshold` aggregators must be collected to update the PoA setup.
