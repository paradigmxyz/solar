//@ codegen-matrix: standard rewritten
//@[rewritten] compile-flags: -O gas -Zdump=mir
//@ compile-flags: -Zllm-optimize=script -Zllm-trace
//@ compile-flags: -Zllm-script=../../tests/ui/codegen/mir/llm-optimize/auxiliary/triangle.script
//@ run-call: triangle 0 => 0
//@ run-call: triangle 1 => 0
//@ run-call: triangle 2 => 1
//@ run-call: triangle 10 => 45
//@ run-call: triangle 1000 => 499500
//@ run-call: triangles 3, 4 => 9
// Gas and size builds replace the loop with the scripted closed form, which `rewritten` shows,
// and the calls run the result; builds without optimization keep the loop and agree.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Triangle {
    function triangle(uint256 n) external pure returns (uint256) {
        return sumBelow(n);
    }

    function triangles(uint256 a, uint256 b) external pure returns (uint256) {
        unchecked {
            return sumBelow(a) + sumBelow(b);
        }
    }

    function sumBelow(uint256 n) internal pure returns (uint256 s) {
        unchecked {
            for (uint256 i; i < n; ++i) {
                s += i;
            }
        }
    }
}
