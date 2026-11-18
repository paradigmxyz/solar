# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.1](https://github.com/paradigmxyz/solar/releases/tag/v0.3.1)

### Bug Fixes

- [codegen] Cap the heap prefix guard at the heap start ([#2694](https://github.com/paradigmxyz/solar/issues/2694))
- [codegen] Bound calldata copies to memory by an unsigned address ([#2548](https://github.com/paradigmxyz/solar/issues/2548))
- [codegen] Keep the heap above reachable absolute local regions ([#2403](https://github.com/paradigmxyz/solar/issues/2403))
- [mir] Keep immutable stores that constructors read ([#2399](https://github.com/paradigmxyz/solar/issues/2399))
- [codegen] Bound nested offsets when copying calldata to memory ([#2397](https://github.com/paradigmxyz/solar/issues/2397))

### Features

- [codegen] Add the solar-cold NatSpec tag ([#2237](https://github.com/paradigmxyz/solar/issues/2237))

### Performance

- [codegen] Encode calldata returns in place ([#2575](https://github.com/paradigmxyz/solar/issues/2575))
- [codegen] Canonicalize width tests, spell them late ([#2224](https://github.com/paradigmxyz/solar/issues/2224))

## [0.3.0](https://github.com/paradigmxyz/solar/releases/tag/v0.3.0)

### Features

- Add the MIR interpreter and run-call check ([#1759](https://github.com/paradigmxyz/solar/issues/1759))
- [ir] Comment function entries and block predecessors in text dumps ([#2229](https://github.com/paradigmxyz/solar/issues/2229))

### Performance

- [codegen] Skip zero fills of untouched heap ([#2236](https://github.com/paradigmxyz/solar/issues/2236))
- [codegen] Treat cold code as bytes ([#2230](https://github.com/paradigmxyz/solar/issues/2230))
- [codegen] Join ABI range checks into one abort test ([#2205](https://github.com/paradigmxyz/solar/issues/2205))
- [mir] Test checked power-of-two products by their high bits ([#2204](https://github.com/paradigmxyz/solar/issues/2204))

