# Davinci storage boundary

S0 (`vize_s0`, retained package id `vize_carton`) is the storage vocabulary for
Davinci stage code. This keeps representation decisions visible at one layer
instead of letting each S1/S2 consumer select a different standard-library
type.

| Need                     | Type                              | Rule                                                                                         |
| ------------------------ | --------------------------------- | -------------------------------------------------------------------------------------------- |
| owned text               | `vize_s0::String`                 | This is `CompactString`; do not name `std::string::String` in stage libraries.               |
| arena-owned sequence     | `vize_s0::Vec`                    | Use for Drop-free IR data allocated with S0.                                                 |
| small scratch sequence   | `vize_s0::SmallVec`               | Use when a measured or grammatical inline bound exists; test both inline and spill behavior. |
| unbounded owned sequence | `alloc::vec::Vec`                 | Retain only in the exact reviewed inventory below; new or removed sites update the ledger.   |
| hash collection          | `vize_s0::{FxHashMap, FxHashSet}` | Do not name a `std::collections` hash type in stage libraries.                               |

The `davinci-opt` files under `crates/vize_davinci/src/bin/davinci-opt/` are an
explicit host edge. They may use `std` for paths, environment, filesystem, I/O,
and exit codes. That exception does not extend to `vize_davinci` library code
or to S1, S2, and S1-to-S2 libraries. Importing or aliasing the `std`, `vec`,
or `collections` modules does not bypass the boundary.

## Retained `alloc::vec::Vec` inventory

The four library trees in the reviewed inventory contain 78 production files,
90 direct `alloc::vec::Vec` paths, and 312 bound `Vec`/`StdVec` uses. "Direct"
counts imports and fully-qualified paths; "bound" counts every type,
constructor, and method path reached through a direct `Vec` import or alias.
The executable ledger requires strict equality, so both growth and reduction
must update the file row and aggregate evidence in the same change.

| Category | Files | Direct paths | Bound uses | Reason                                                                                                             |
| -------- | ----: | -----------: | ---------: | ------------------------------------------------------------------------------------------------------------------ |
| contract |    13 |           24 |         65 | Owned Folio and S2 serialization data has input-defined cardinality and forms a stable contract.                   |
| analysis |     7 |            8 |         18 | Diagnostics, side tables, filters, and verifier results grow with the input; no inline bound is established.       |
| lower    |    12 |           12 |         47 | Lowering worklists and owned results grow with source-tree shape. Bounded substructures may migrate independently. |
| pass     |    14 |           14 |         55 | Facts, provenance, and traversal worklists grow with the number of operations.                                     |
| emit     |    32 |           32 |        127 | Ordered output buffers and collected emission inputs grow with the document.                                       |

This is not an endorsement of every retained allocation. A focused change may
replace a site with `SmallVec` after measuring a bound; that change lowers the
exact ledger and aggregate in the same commit, making reintroduction fail.
Mechanical conversion of source-sized buffers is not a goal because it can
move large payloads onto the stack or add spill bookkeeping without reducing
allocations.

The second `alloc::vec::Vec` import in `side_table.rs` is `#[cfg(test)]`
size/test evidence. The scanner excludes the complete attributed item or
module, so it cannot inflate production totals.

## Exact owned-storage inventory by scope

Each row is derived from the per-file
[`storage-inventory.tsv`](./storage-inventory.tsv) ratchet. Zero rows matter:
in particular, any production `alloc::string::String` path creates a new file
or count and fails the gate instead of becoming a `no_std` escape from S0.

| Scope    | Type                    | Files | Direct paths | Bound uses |
| -------- | ----------------------- | ----: | -----------: | ---------: |
| infra    | `alloc::vec::Vec`       |    10 |           10 |         39 |
| infra    | `alloc::string::String` |     0 |            0 |          0 |
| infra    | `vize_s0::String`       |    12 |           12 |         83 |
| infra    | `vize_s0::Vec`          |     0 |            0 |          0 |
| infra    | `vize_s0::SmallVec`     |     0 |            0 |          0 |
| s1       | `alloc::vec::Vec`       |     0 |            0 |          0 |
| s1       | `alloc::string::String` |     0 |            0 |          0 |
| s1       | `vize_s0::String`       |     0 |            0 |          0 |
| s1       | `vize_s0::Vec`          |     5 |            5 |         22 |
| s1       | `vize_s0::SmallVec`     |     0 |            0 |          0 |
| s2       | `alloc::vec::Vec`       |    10 |           22 |         44 |
| s2       | `alloc::string::String` |     0 |            0 |          0 |
| s2       | `vize_s0::String`       |    11 |           11 |         55 |
| s2       | `vize_s0::Vec`          |     9 |            9 |         17 |
| s2       | `vize_s0::SmallVec`     |     0 |            0 |          0 |
| s1_to_s2 | `alloc::vec::Vec`       |    58 |           58 |        229 |
| s1_to_s2 | `alloc::string::String` |     0 |            0 |          0 |
| s1_to_s2 | `vize_s0::String`       |    84 |           88 |        429 |
| s1_to_s2 | `vize_s0::Vec`          |    15 |           16 |         69 |
| s1_to_s2 | `vize_s0::SmallVec`     |     4 |            4 |          9 |

`tests/tooling/davinci-storage-policy.test.ts` masks comments, literals, and
`#[cfg(test)]` items; resolves root, self, group, module, and raw aliases; and
checks every production file, category, scope, and owned-storage type for exact
equality with the TSV and both tables above.
