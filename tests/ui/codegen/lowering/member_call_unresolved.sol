//@compile-flags: -Zdump=evm-ir
// Calls with an erroneous receiver or builtin argument must reuse that error,
// not emit another diagnostic or ICE in codegen.
import {Missing} from "./does-not-exist.sol"; //~ ERROR: file

contract MemberCallUnresolved {
    function f(Missing m) external { //~ ERROR: unresolved symbol `Missing`
        m.push(1);
        keccak256(missing); //~ ERROR: unresolved symbol `missing`
        keccak256(); //~ ERROR: wrong argument count
        abi.encodeWithSelector(); //~ ERROR: wrong argument count
        abi.encode({a: 1}); //~ ERROR: named arguments cannot be used
        keccak256({a: bytes("")}); //~ ERROR: named arguments cannot be used
        assembly {
            pop() //~ ERROR: wrong argument count
            let x := add(1) //~ ERROR: wrong argument count
        }
    }
}
