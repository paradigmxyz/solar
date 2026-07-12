// ported-from: test/libsolidity/syntaxTests/using/using_library_for_library.sol

library L {}
library M {}

using L for M; //~ ERROR: invalid use of library name
using M for L; //~ ERROR: invalid use of library name
using L for L; //~ ERROR: invalid use of library name
