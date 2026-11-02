# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0](https://github.com/paradigmxyz/solar/releases/tag/v0.3.0)

### Features

- [ir] Comment function entries and block predecessors in text dumps ([#2229](https://github.com/paradigmxyz/solar/issues/2229))
- Add the solar NatSpec tags ([#2101](https://github.com/paradigmxyz/solar/issues/2101))
- [standard-json] Key cborAuxdata by contract ([#2149](https://github.com/paradigmxyz/solar/issues/2149))
- [standard-json] Report CBOR auxdata positions ([#2142](https://github.com/paradigmxyz/solar/issues/2142))
- Add solar-evmir and an EVM IR interpreter ([#1782](https://github.com/paradigmxyz/solar/issues/1782))

### Performance

- [codegen] Treat cold code as bytes ([#2230](https://github.com/paradigmxyz/solar/issues/2230))
- [codegen] Push zero as RETURNDATASIZE before PUSH0 ([#2207](https://github.com/paradigmxyz/solar/issues/2207))
- [evm] Bound call continuations for compact-pushes ([#2120](https://github.com/paradigmxyz/solar/issues/2120))

