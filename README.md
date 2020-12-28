# clerkb

Clerkb provides [Proof of Authority](https://en.wikipedia.org/wiki/Proof_of_authority) mechanism for [Nervos CKB](https://www.nervos.org/). It is designed as 2 components working together:

* 2 smart contracts used as lock scripts on CKB to validate logic on chain.
* A TypeScript based module for integrating PoA into your generator code.

The name comes from a combination of `clerk`, and CKB.

## Design

A `PoASetup` construct specifies the behavior of clerkb:

```typescript
export interface PoASetup {
  identity_size: number;
  interval_uses_seconds: boolean;
  identities: Array<HexString>;
  aggregator_change_threshold: number;
  subblock_intervals: number;
  subblocks_per_interval: number;
}
```

One is free to specify the configurations here:

* The number of aggregators used in PoA. Each aggregator is denoted by one entry in `identitites` array. Clerkb is designed to support at most 255 aggregators.
* Each aggregator can issue layer 2 blocks(represented as CKB transactions) in its round. A round has a time limit denoted by `subblocks_per_interval`. The limit can be expressed either as timestamps, or block numbers.
* `identitites` determine the order in which aggregator gets its round. When the round for one aggregator expires, the next aggregator denoted by `identitites` array gets the round. When the last aggregator in `identitites` expires its round, clerkb restarts with the first aggregator in `identitites` array.
* Each aggregator can issue as many as `subblocks_per_interval` layer 2 blocks(represented as CKB transactions) in each round.
* The contents of `identitites` array, are actually script hashes, or prefixes of script hashes(denoted by `identity_size`). Here clerkb uses the same technical as owner locks in [sUDT](https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0025-simple-udt/0025-simple-udt.md): an aggregator can unlock a cell governed by the PoA lock, as long as current transaction has an input cell, whose lock script hash is identical to the entry specified in `identitites` array.
