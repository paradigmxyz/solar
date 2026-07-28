//@compile-flags: -Zcodegen -Zdump=evm-ir
// Calls with an erroneous receiver or builtin argument must reuse that error,
// not emit another diagnostic or ICE in codegen.
import {Missing} from "./does-not-exist.sol"; //~ ERROR: file

contract MemberCallUnresolved {
    function f(Missing m) external { //~ ERROR: unresolved symbol `Missing`
        m.push(1);
        keccak256(missing); //~ ERROR: unresolved symbol `missing`
        keccak256(); //~ ERROR: wrong argument count
        //~^ ERROR: wrong number of arguments for builtin `keccak256`
        abi.encodeWithSelector(); //~ ERROR: wrong argument count
        //~^ ERROR: wrong number of arguments for builtin `encodeWithSelector`
        assembly {
            pop() //~ ERROR: wrong argument count
            //~^ ERROR: wrong number of arguments for Yul builtin `pop`
            let x := add(1) //~ ERROR: wrong argument count
            //~^ ERROR: wrong number of arguments for Yul builtin `add`
        }
    }
}
