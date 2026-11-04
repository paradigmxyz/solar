//@ compile-flags: --emit=ast -Zno-resolve-imports

// A source whose imports were not resolved has no AST, since an import directive needs the source
// that it imports.

import "./does_not_exist.sol";

contract C {}
