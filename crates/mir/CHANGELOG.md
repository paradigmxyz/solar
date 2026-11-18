# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.1](https://github.com/paradigmxyz/solar/releases/tag/v0.3.1)

### Bug Fixes

- [codegen] Cap the heap prefix guard at the heap start ([#2694](https://github.com/paradigmxyz/solar/issues/2694))
- [codegen] Leave if arm exits without a location ([#2699](https://github.com/paradigmxyz/solar/issues/2699))
- [codegen] Map bypassed arm assignments to their edge ([#2668](https://github.com/paradigmxyz/solar/issues/2668))
- [codegen] Map copies that feed a phi to their assignments ([#2681](https://github.com/paradigmxyz/solar/issues/2681))
- [codegen] Bound calldata copies to memory by an unsigned address ([#2548](https://github.com/paradigmxyz/solar/issues/2548))
- [codegen] Map phi operand copies to their assignments ([#2642](https://github.com/paradigmxyz/solar/issues/2642))
- [codegen] Ignore unreachable heap prefix steps ([#2650](https://github.com/paradigmxyz/solar/issues/2650))
- [inline] Keep semantic metadata on inlined instructions ([#2560](https://github.com/paradigmxyz/solar/issues/2560))
- [codegen] Merge coalesced copy origins and keep events unique ([#2481](https://github.com/paradigmxyz/solar/issues/2481))
- [debuginfo] Share one DebugLoc between MIR and EVM IR ([#2483](https://github.com/paradigmxyz/solar/issues/2483))
- [codegen] Map ABI wrapper code to its declaration ([#2462](https://github.com/paradigmxyz/solar/issues/2462))
- [codegen] Give the dispatcher the contract's origin ([#2461](https://github.com/paradigmxyz/solar/issues/2461))
- [codegen] Mark synthesized helpers as compiler generated ([#2457](https://github.com/paradigmxyz/solar/issues/2457))
- [codegen] Keep the heap above reachable absolute local regions ([#2403](https://github.com/paradigmxyz/solar/issues/2403))
- [mir] Keep the label origin of cloned blocks ([#2425](https://github.com/paradigmxyz/solar/issues/2425))
- [debuginfo] Keep every origin of shared instructions ([#2373](https://github.com/paradigmxyz/solar/issues/2373))
- [mir] Keep immutable stores that constructors read ([#2399](https://github.com/paradigmxyz/solar/issues/2399))
- [codegen] Bound nested offsets when copying calldata to memory ([#2397](https://github.com/paradigmxyz/solar/issues/2397))
- [ir] Report oversized integer literals at the literal ([#2329](https://github.com/paradigmxyz/solar/issues/2329))

### Features

- [mir] Record the origin of each block's label ([#2415](https://github.com/paradigmxyz/solar/issues/2415))
- [mir] Canonicalize operand order at construction ([#2245](https://github.com/paradigmxyz/solar/issues/2245))
- [codegen] Add the solar-cold NatSpec tag ([#2237](https://github.com/paradigmxyz/solar/issues/2237))

### Performance

- [codegen] Scope dangling alias cleanup ([#2645](https://github.com/paradigmxyz/solar/issues/2645))
- [codegen] Encode calldata returns in place ([#2575](https://github.com/paradigmxyz/solar/issues/2575))
- [codegen] Stage calldata hash inputs ([#2504](https://github.com/paradigmxyz/solar/issues/2504))
- [codegen] Treat try/catch catch clauses as cold ([#2238](https://github.com/paradigmxyz/solar/issues/2238))
- [codegen] Canonicalize width tests, spell them late ([#2224](https://github.com/paradigmxyz/solar/issues/2224))
- [codegen] Encode constructor args in place after initcode ([#2320](https://github.com/paradigmxyz/solar/issues/2320))
- [codegen] Encode storage bytes straight from storage ([#2315](https://github.com/paradigmxyz/solar/issues/2315))
- [codegen] Copy packed word arrays as one block ([#2346](https://github.com/paradigmxyz/solar/issues/2346))

## [0.3.0](https://github.com/paradigmxyz/solar/releases/tag/v0.3.0)

### Features

- Add the MIR interpreter and run-call check ([#1759](https://github.com/paradigmxyz/solar/issues/1759))
- [ir] Comment function entries and block predecessors in text dumps ([#2229](https://github.com/paradigmxyz/solar/issues/2229))

### Performance

- [codegen] Skip zero fills of untouched heap ([#2236](https://github.com/paradigmxyz/solar/issues/2236))
- [codegen] Treat cold code as bytes ([#2230](https://github.com/paradigmxyz/solar/issues/2230))
- [codegen] Join ABI range checks into one abort test ([#2205](https://github.com/paradigmxyz/solar/issues/2205))
- [mir] Test checked power-of-two products by their high bits ([#2204](https://github.com/paradigmxyz/solar/issues/2204))

