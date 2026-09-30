# solar-std

Solidity sources of the Solar compiler's standard library.

The modules live under `solidity/`, at their import paths below the reserved
`solar:core/` prefix: `import {Bytes} from "solar:core/Bytes.sol";` reads
`solidity/Bytes.sol`. The compiler embeds the text and never reads these files.
A project that also builds with another compiler can remap `solar:core/` to a
copy of the directory; this compiler accepts such a copy only when its text is
exactly the module's.

`gen_modules.py` generates the modules whose sources say so.
