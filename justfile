# Run tests and generate coverage report for VS Code Coverage Gutters
coverage:
    #!/usr/bin/env bash
    set -euo pipefail
    
    # Clean up any existing coverage files
    rm -f lcov.info
    
    # Use cargo-llvm-cov to generate LCOV format coverage report
    # This handles LLVM version compatibility automatically
    cargo llvm-cov --lcov --output-path lcov.info

# Generate and display coverage summary in terminal
coverage-summary:
    cargo llvm-cov --summary-only

# Generate HTML coverage report for detailed browsing
coverage-html:
    #!/usr/bin/env bash
    set -euo pipefail
    
    cargo llvm-cov --html --output-dir coverage-html
