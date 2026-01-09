//@compile-flags: -Ztypeck

type Int is int128;

function add(Int x, Int y) pure returns (Int) {}
function sub(Int x, Int y) pure returns (Int) {}
function neg(Int x) pure returns (Int) {}
function bitnot(Int x) pure returns (Int) {}
function eq(Int x, Int y) pure returns (bool) {}
function lt(Int x, Int y) pure returns (bool) {}

using {add as +, sub as -, neg as -, bitnot as ~, eq as ==, lt as <} for Int global;
