//@compile-flags: -Zcodegen --emit=bin-runtime
// ported-from: src/utils/LibBytes.sol

// A parameter reassigned in inline assembly, the solady `LibBytes.indexOf`
// shape: `subject` advances inside two mutually-exclusive Yul search loops.
// Bound as a plain SSA value it could not merge across the loop back edge or
// between the sibling loops, so the search read a definition that could never
// reach it. It now lives in a frame slot like a reassigned local.
//
// The slot address spans the parameter and return areas, so the store is
// staged until every parameter and return is registered; computing it mid-loop
// resolved to a different slot than the body's reads. Verified against solc.
library LibBytes {
    function indexOf(bytes memory subject, bytes memory needle, uint256 from)
        internal
        pure
        returns (uint256 result)
    {
        assembly {
            result := not(0)
            for { let subjectLen := mload(subject) } 1 {} {
                if iszero(mload(needle)) {
                    result := from
                    if iszero(gt(from, subjectLen)) { break }
                    result := subjectLen
                    break
                }
                let needleLen := mload(needle)
                let subjectStart := add(subject, 0x20)

                subject := add(subjectStart, from)
                let end := add(sub(add(subjectStart, subjectLen), needleLen), 1)
                let m := shl(3, sub(0x20, and(needleLen, 0x1f)))
                let s := mload(add(needle, 0x20))

                if iszero(and(lt(subject, end), lt(from, subjectLen))) { break }

                if iszero(lt(needleLen, 0x20)) {
                    for { let h := keccak256(add(needle, 0x20), needleLen) } 1 {} {
                        if iszero(shr(m, xor(mload(subject), s))) {
                            if eq(keccak256(subject, needleLen), h) {
                                result := sub(subject, subjectStart)
                                break
                            }
                        }
                        subject := add(subject, 1)
                        if iszero(lt(subject, end)) { break }
                    }
                    break
                }
                for {} 1 {} {
                    if iszero(shr(m, xor(mload(subject), s))) {
                        result := sub(subject, subjectStart)
                        break
                    }
                    subject := add(subject, 1)
                    if iszero(lt(subject, end)) { break }
                }
                break
            }
        }
    }
}

contract YulParamReassignment {
    function indexOf(bytes memory subject, bytes memory needle, uint256 from)
        external
        pure
        returns (uint256)
    {
        return LibBytes.indexOf(subject, needle, from);
    }
}
