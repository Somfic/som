# Run tests and generate coverage report for VS Code Coverage Gutters
coverage:
    #!/usr/bin/env bash
    set -euo pipefail
    
    # Clean up any existing coverage files
    rm -f lcov.info
    
    echo "Running tests and generating coverage report using cargo-llvm-cov..."
    
    # Use cargo-llvm-cov to generate LCOV format coverage report
    # This handles LLVM version compatibility automatically
    cargo llvm-cov --lcov --output-path lcov.info
    
    echo "Coverage report generated: lcov.info"
    echo ""
    echo "To view in VS Code:"
    echo "1. Install the 'Coverage Gutters' extension if not already installed"
    echo "2. Open Command Palette (Cmd+Shift+P)"
    echo "3. Run 'Coverage Gutters: Display Coverage'"
    echo "4. The extension should automatically find lcov.info"
    echo ""
    echo "You can also view a summary in the terminal with:"
    echo "  just coverage-summary"

# Generate and display coverage summary in terminal
coverage-summary:
    cargo llvm-cov --summary-only

# Generate coverage report for specific tests only
coverage-passing:
    #!/usr/bin/env bash
    set -euo pipefail
    
    # Clean up any existing coverage files
    rm -f lcov.info
    
    echo "Running selected tests and generating coverage report..."
    
    # Run coverage for lexer tests (known to pass)
    cargo llvm-cov --lcov --output-path lcov.info test lexer::tests
    
    # Add arithmetic tests (also known to pass)
    cargo llvm-cov --lcov --output-path lcov.info --no-clean test tests::arithmetic
    
    echo "Coverage report generated: lcov.info"
    echo ""
    echo "To view in VS Code:"
    echo "1. Install the 'Coverage Gutters' extension if not already installed"
    echo "2. Open Command Palette (Cmd+Shift+P)" 
    echo "3. Run 'Coverage Gutters: Display Coverage'"

# Generate HTML coverage report for detailed browsing
coverage-html:
    #!/usr/bin/env bash
    set -euo pipefail
    
    echo "Generating HTML coverage report..."
    cargo llvm-cov --html --output-dir coverage-html
    
    echo "HTML coverage report generated in: coverage-html/"
    echo "Open coverage-html/index.html in your browser to view detailed coverage"

# Legacy coverage command (simplified)
cover:
    CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='cargo-test-%p-%m.profraw' cargo test
