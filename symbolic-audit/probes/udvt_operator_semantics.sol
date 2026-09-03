type Small is uint8;
type Word is uint256;

using {eqMod3 as ==, addAsMul as +, ltRev as <, negPlusOne as -} for Small global;
using {wordEq as ==} for Word global;

function eqMod3(Small a, Small b) pure returns (bool) { return Small.unwrap(a) % 3 == Small.unwrap(b) % 3; }
function addAsMul(Small a, Small b) pure returns (Small) { unchecked { return Small.wrap(Small.unwrap(a) * Small.unwrap(b)); } }
function ltRev(Small a, Small b) pure returns (bool) { return Small.unwrap(a) > Small.unwrap(b); }
function negPlusOne(Small a) pure returns (Small) { unchecked { return Small.wrap(Small.unwrap(a) + 1); } }
function wordEq(Word a, Word b) pure returns (bool) { return Word.unwrap(a) / 2 == Word.unwrap(b) / 2; }

contract UdvtOperatorSemantics {
    function eq(uint256 a, uint256 b) external pure returns (bool) { return Small.wrap(uint8(a)) == Small.wrap(uint8(b)); }
    function add(uint256 a, uint256 b) external pure returns (uint8) { return Small.unwrap(Small.wrap(uint8(a)) + Small.wrap(uint8(b))); }
    function lt(uint256 a, uint256 b) external pure returns (bool) { return Small.wrap(uint8(a)) < Small.wrap(uint8(b)); }
    function neg(uint256 a) external pure returns (uint8) { return Small.unwrap(-Small.wrap(uint8(a))); }
    function wordEqTest(uint256 a, uint256 b) external pure returns (bool) { return Word.wrap(a) == Word.wrap(b); }
    function dirtyEq(uint256 a, uint256 b) external pure returns (bool) { Small x; Small y; assembly { x := a y := b } return x == y; }
}
