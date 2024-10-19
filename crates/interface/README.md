# solar-interface

Source positions, diagnostics, and related helper functions.

Important concepts in this module include:

- the *span*, represented by [`Span`] and related types;
- source code as represented by a [`SourceMap`]; and
- interned strings, represented by [`Symbol`]s, with some common symbols available statically in
  the [`sym`] module.
