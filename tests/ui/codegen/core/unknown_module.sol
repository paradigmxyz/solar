//@ compile-flags: -Ogas

// The reserved prefix is resolved from the compiler's own module list, so an
// unknown path under it is an error rather than a missing file.
import {Nope} from "solar:core/v1/Nope.sol"; //~ ERROR: unknown compiler module `solar:core/v1/Nope.sol`

contract Test {}
