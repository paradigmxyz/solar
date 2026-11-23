# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.1](https://github.com/paradigmxyz/solar/releases/tag/v0.3.1)

### Bug Fixes

- [codegen] Mark call entries and returns once per call ([#2528](https://github.com/paradigmxyz/solar/issues/2528))
- [codegen] Merge coalesced copy origins and keep events unique ([#2481](https://github.com/paradigmxyz/solar/issues/2481))
- [debuginfo] Share one DebugLoc between MIR and EVM IR ([#2483](https://github.com/paradigmxyz/solar/issues/2483))
- [debuginfo] Keep every origin of shared instructions ([#2373](https://github.com/paradigmxyz/solar/issues/2373))
- [codegen] Preserve source maps and coverage ([#2345](https://github.com/paradigmxyz/solar/issues/2345))
- [ir] Report oversized integer literals at the literal ([#2329](https://github.com/paradigmxyz/solar/issues/2329))

### Features

- [codegen] Carry label origins from MIR to JUMPDESTs ([#2416](https://github.com/paradigmxyz/solar/issues/2416))

## [0.3.0](https://github.com/paradigmxyz/solar/releases/tag/v0.3.0)

### Features

- [ir] Comment function entries and block predecessors in text dumps ([#2229](https://github.com/paradigmxyz/solar/issues/2229))

### Performance

- [codegen] Treat cold code as bytes ([#2230](https://github.com/paradigmxyz/solar/issues/2230))

