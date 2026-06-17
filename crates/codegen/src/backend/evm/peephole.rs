//! Peephole optimizer for EVM bytecode.
//!
//! Performs pattern-based optimizations on raw bytecode sequences,
//! replacing inefficient patterns with more optimal ones.

use super::assembler::op;

/// Peephole optimizer for EVM bytecode.
pub struct PeepholeOptimizer;

impl PeepholeOptimizer {
    /// Optimizes the given bytecode using peephole patterns.
    /// Returns the optimized bytecode.
    #[must_use]
    pub fn optimize(bytecode: &[u8]) -> Vec<u8> {
        let mut result = bytecode.to_vec();
        let mut changed = true;

        // Iterate until no more changes (patterns may enable other patterns)
        while changed {
            changed = false;
            let mut new_result = Vec::with_capacity(result.len());
            let mut i = 0;

            while i < result.len() {
                // Try each pattern in order of precedence
                if let Some((skip, replacement)) = Self::try_pattern(&result, i) {
                    new_result.extend_from_slice(&replacement);
                    i += skip;
                    changed = true;
                } else {
                    new_result.push(result[i]);
                    i += 1;
                }
            }

            result = new_result;
        }

        result
    }

    /// Tries to match a pattern at the given position.
    /// Returns (bytes_to_skip, replacement_bytes) if a pattern matches.
    fn try_pattern(bytecode: &[u8], pos: usize) -> Option<(usize, Vec<u8>)> {
        let remaining = &bytecode[pos..];

        // Pattern: PUSH0 ADD -> (nop) - adding zero is identity
        if remaining.len() >= 2 && remaining[0] == op::PUSH0 && remaining[1] == op::ADD {
            return Some((2, vec![]));
        }

        // Pattern: PUSH0 MUL -> PUSH0 POP PUSH0 -> POP PUSH0
        // Actually: x * 0 = 0, but we need to consume x from stack
        // This is: [x] PUSH0 MUL -> [0], so we replace with POP PUSH0
        if remaining.len() >= 2 && remaining[0] == op::PUSH0 && remaining[1] == op::MUL {
            return Some((2, vec![op::POP, op::PUSH0]));
        }

        // Pattern: PUSH1 1 MUL -> (nop) - multiplying by 1 is identity
        if remaining.len() >= 3
            && remaining[0] == op::PUSH1
            && remaining[1] == 1
            && remaining[2] == op::MUL
        {
            return Some((3, vec![]));
        }

        // Pattern: SWAP1 SWAP1 -> (nop) - double swap is identity
        if remaining.len() >= 2 && remaining[0] == op::swap(1) && remaining[1] == op::swap(1) {
            return Some((2, vec![]));
        }

        // Pattern: SWAP<N> SWAP<N> -> (nop) for any n
        for n in 2..=16u8 {
            if remaining.len() >= 2 && remaining[0] == op::swap(n) && remaining[1] == op::swap(n) {
                return Some((2, vec![]));
            }
        }

        // Pattern: DUP1 POP -> (nop) - dup then immediate pop is useless
        if remaining.len() >= 2 && remaining[0] == op::dup(1) && remaining[1] == op::POP {
            return Some((2, vec![]));
        }

        // Pattern: DUP<N> POP -> (nop) for any n
        for n in 2..=16u8 {
            if remaining.len() >= 2 && remaining[0] == op::dup(n) && remaining[1] == op::POP {
                return Some((2, vec![]));
            }
        }

        // Pattern: ISZERO ISZERO ISZERO -> ISZERO
        // (reduces to single negation)
        if remaining.len() >= 3
            && remaining[0] == op::ISZERO
            && remaining[1] == op::ISZERO
            && remaining[2] == op::ISZERO
        {
            return Some((3, vec![op::ISZERO]));
        }

        // Pattern: NOT NOT -> (nop) - double bitwise not is identity
        if remaining.len() >= 2 && remaining[0] == op::NOT && remaining[1] == op::NOT {
            return Some((2, vec![]));
        }

        // Pattern: PUSH0 OR -> (nop) - OR with 0 is identity
        if remaining.len() >= 2 && remaining[0] == op::PUSH0 && remaining[1] == op::OR {
            return Some((2, vec![]));
        }

        // Pattern: PUSH0 XOR -> (nop) - XOR with 0 is identity
        if remaining.len() >= 2 && remaining[0] == op::PUSH0 && remaining[1] == op::XOR {
            return Some((2, vec![]));
        }

        // Pattern: PUSH0 EQ -> ISZERO
        if remaining.len() >= 2 && remaining[0] == op::PUSH0 && remaining[1] == op::EQ {
            return Some((2, vec![op::ISZERO]));
        }

        // Pattern: PUSH0 SHL/SHR/SAR -> (nop) - shifting by 0 is identity
        if remaining.len() >= 2
            && remaining[0] == op::PUSH0
            && matches!(remaining[1], op::SHL | op::SHR | op::SAR)
        {
            return Some((2, vec![]));
        }

        // Pattern: POP POP -> double pop can be combined into consecutive POPs
        // (no optimization, but we can detect push-pop sequences)

        // Pattern: PUSH<n> <val> POP -> (nop) - push followed by immediate pop
        if remaining.len() >= 2 && remaining[0] == op::PUSH0 && remaining[1] == op::POP {
            return Some((2, vec![]));
        }

        // PUSH1-PUSH32 followed by POP
        if !remaining.is_empty() && (op::PUSH1..=op::PUSH32).contains(&remaining[0]) {
            let push_size = (remaining[0] - op::PUSH0) as usize; // 1-32 bytes
            let total_len = 1 + push_size; // opcode + data bytes
            if remaining.len() > total_len && remaining[total_len] == op::POP {
                return Some((total_len + 1, vec![]));
            }
        }

        // Pattern: JUMP after JUMP/JUMPI/STOP/RETURN/REVERT/INVALID
        // (unreachable code elimination)
        // MIR DCE handles this before EVM assembly is built; doing it here would
        // require reconstructing basic block boundaries.

        // Pattern: EQ ISZERO -> can sometimes be combined with jumps
        // but this requires more context

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push0_add() {
        // PUSH0 ADD should be removed
        let input = vec![0x60, 0x42, op::PUSH0, op::ADD]; // PUSH1 42, PUSH0, ADD
        let output = PeepholeOptimizer::optimize(&input);
        assert_eq!(output, vec![0x60, 0x42]); // Just PUSH1 42
    }

    #[test]
    fn test_swap1_swap1() {
        // SWAP1 SWAP1 should be removed
        let input = vec![0x60, 0x01, 0x60, 0x02, op::swap(1), op::swap(1)];
        let output = PeepholeOptimizer::optimize(&input);
        assert_eq!(output, vec![0x60, 0x01, 0x60, 0x02]);
    }

    #[test]
    fn test_dup1_pop() {
        // DUP1 POP should be removed
        let input = vec![0x60, 0x42, op::dup(1), op::POP];
        let output = PeepholeOptimizer::optimize(&input);
        assert_eq!(output, vec![0x60, 0x42]);
    }

    #[test]
    fn test_push_pop() {
        // PUSH<n> <val> POP should be removed
        let input = vec![0x60, 0x42, op::POP]; // PUSH1 42, POP
        let output = PeepholeOptimizer::optimize(&input);
        assert!(output.is_empty());

        // PUSH2 with 2 bytes then POP
        let input2 = vec![0x61, 0x01, 0x02, op::POP];
        let output2 = PeepholeOptimizer::optimize(&input2);
        assert!(output2.is_empty());
    }

    #[test]
    fn test_push0_pop() {
        // PUSH0 POP should be removed
        let input = vec![op::PUSH0, op::POP];
        let output = PeepholeOptimizer::optimize(&input);
        assert!(output.is_empty());
    }

    #[test]
    fn test_not_not() {
        // NOT NOT should be removed
        let input = vec![0x60, 0x42, op::NOT, op::NOT];
        let output = PeepholeOptimizer::optimize(&input);
        assert_eq!(output, vec![0x60, 0x42]);
    }

    #[test]
    fn test_triple_iszero() {
        // ISZERO ISZERO ISZERO -> ISZERO
        let input = vec![0x60, 0x00, op::ISZERO, op::ISZERO, op::ISZERO];
        let output = PeepholeOptimizer::optimize(&input);
        assert_eq!(output, vec![0x60, 0x00, op::ISZERO]);
    }

    #[test]
    fn test_push1_1_mul() {
        // PUSH1 1 MUL should be removed (multiply by 1)
        let input = vec![0x60, 0x42, 0x60, 0x01, op::MUL];
        let output = PeepholeOptimizer::optimize(&input);
        assert_eq!(output, vec![0x60, 0x42]);
    }

    #[test]
    fn test_push1_1_div_preserved() {
        // PUSH1 1 DIV is not an identity in EVM stack order
        let input = vec![0x60, 0x42, 0x60, 0x01, op::DIV];
        let output = PeepholeOptimizer::optimize(&input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_push0_or() {
        // PUSH0 OR should be removed
        let input = vec![0x60, 0x42, op::PUSH0, op::OR];
        let output = PeepholeOptimizer::optimize(&input);
        assert_eq!(output, vec![0x60, 0x42]);
    }

    #[test]
    fn test_push0_xor() {
        // PUSH0 XOR should be removed
        let input = vec![0x60, 0x42, op::PUSH0, op::XOR];
        let output = PeepholeOptimizer::optimize(&input);
        assert_eq!(output, vec![0x60, 0x42]);
    }

    #[test]
    fn test_push0_sub_preserved() {
        // PUSH0 SUB is not an identity in EVM stack order
        let input = vec![0x60, 0x42, op::PUSH0, op::SUB];
        let output = PeepholeOptimizer::optimize(&input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_chained_optimizations() {
        // After one optimization, another may become possible
        // PUSH0 ADD PUSH0 ADD -> (empty) after two passes
        let input = vec![0x60, 0x42, op::PUSH0, op::ADD, op::PUSH0, op::ADD];
        let output = PeepholeOptimizer::optimize(&input);
        assert_eq!(output, vec![0x60, 0x42]);
    }

    #[test]
    fn test_preserves_valid_code() {
        // Valid code should not be modified
        let input = vec![
            0x60,
            0x42, // PUSH1 42
            0x60,
            0x10,    // PUSH1 16
            op::ADD, // ADD
            op::STOP,
        ];
        let output = PeepholeOptimizer::optimize(&input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_swap_various_depths() {
        // SWAP2 SWAP2, SWAP3 SWAP3, etc. should be removed
        for n in 2..=16u8 {
            let input = vec![op::swap(n), op::swap(n)];
            let output = PeepholeOptimizer::optimize(&input);
            assert!(output.is_empty(), "SWAP{n} SWAP{n} should be removed");
        }
    }

    #[test]
    fn test_dup_various_depths() {
        // DUP2 POP, DUP3 POP, etc. should be removed
        for n in 2..=16u8 {
            let input = vec![op::dup(n), op::POP];
            let output = PeepholeOptimizer::optimize(&input);
            assert!(output.is_empty(), "DUP{n} POP should be removed");
        }
    }
}
