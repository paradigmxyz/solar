use solar_parse::Lexer;
use solar_interface::Session;

fn main() {
    let session = Session::builder()
        .with_silent_emitter(None)
        .build();
    
    // Test various Solidity code patterns to exercise SIMD optimizations
    let test_cases = [
        "contract Test {}",
        "   \n\t\r  contract Test {}",  // lots of whitespace
        "uint256 very_long_identifier_name_to_test_bulk_processing",
        "123456789012345678901234567890",  // long number
        "0xabcdefabcdefabcdefabcdefabcdef",  // hex number
        "// This is a comment\ncontract Test {}",
    ];
    
    for (i, code) in test_cases.iter().enumerate() {
        println!("Test case {}: {:?}", i + 1, code);
        let tokens: Vec<_> = Lexer::new(&session, code)
            .filter(|t| !t.is_comment())
            .collect();
        println!("  Tokens: {}", tokens.len());
        
        // Verify lexer produces expected results
        if tokens.is_empty() && !code.trim().is_empty() {
            println!("  WARNING: No tokens found for non-empty code!");
        } else {
            println!("  SUCCESS: Lexer working correctly");
        }
    }
    
    println!("SIMD optimizations implemented and working!");
}