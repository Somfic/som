# Coverage Guide for SOM Project

This guide explains how to generate and view code coverage reports for the SOM project in VS Code.

## Quick Start

1. **Generate coverage report for stable tests:**

   ```bash
   just coverage-passing
   ```

2. **View coverage in VS Code:**
   - Install the "Coverage Gutters" extension
   - Open Command Palette (Cmd+Shift+P / Ctrl+Shift+P)
   - Run "Coverage Gutters: Display Coverage"

## Available Commands

### `just coverage-passing` (Recommended)

Runs only the tests that are known to pass consistently:

- Lexer tests (tokenization, parsing)  
- Arithmetic expression tests (basic math operations)

This is the most reliable option and provides good coverage of core functionality.

### `just coverage`

Attempts to run all tests and generate coverage. Some tests may fail due to incomplete features, but coverage will still be generated for the code that does execute.

### `just coverage-summary`

Shows a text-based coverage summary in the terminal without generating the LCOV file.

### `just coverage-html`

Generates an HTML coverage report in the `coverage-html/` directory that you can open in a web browser for detailed coverage analysis.

## Coverage File

The coverage data is saved to `lcov.info` in the project root. This file is automatically detected by the Coverage Gutters extension in VS Code.

## VS Code Integration

### Installing Coverage Gutters Extension

1. Open VS Code Extensions panel (Cmd+Shift+X / Ctrl+Shift+X)
2. Search for "Coverage Gutters" by ryanluker
3. Click Install

### Viewing Coverage

After generating a coverage report:

1. **Automatic Detection**: Coverage Gutters should automatically detect the `lcov.info` file
2. **Manual Activation:**
   - Command Palette → "Coverage Gutters: Display Coverage"
   - Or click the "Watch" button in the status bar

### Coverage Visualization

- **Green lines**: Covered by tests
- **Red lines**: Not covered by tests  
- **Orange lines**: Partially covered (e.g., only one branch of an if statement)
- **No highlighting**: Not executable code (comments, empty lines, etc.)

### Additional Commands

- **"Coverage Gutters: Remove Coverage"**: Hide coverage highlighting
- **"Coverage Gutters: Toggle Coverage"**: Show/hide coverage
- **"Coverage Gutters: Preview Coverage Report"**: View detailed report

## Understanding the Coverage Data

The coverage report shows:

- **Line Coverage**: Which lines of code were executed during tests
- **Function Coverage**: Which functions were called
- **Branch Coverage**: Which conditional branches were taken

Focus areas for improving coverage:

1. Add tests for uncovered functions
2. Test both branches of conditional statements
3. Test error handling paths
4. Test edge cases and boundary conditions

## Troubleshooting

### Coverage file not found

- Make sure you've run one of the coverage commands first
- Check that `lcov.info` exists in the project root

### Extension not working

- Restart VS Code after installing Coverage Gutters
- Check that the extension is enabled
- Try manually running "Coverage Gutters: Display Coverage"

### No coverage data shown

- Verify the coverage file was generated successfully
- Some files may have no coverage if they weren't executed during tests
- Check the Output panel for any error messages

## Development Workflow

1. **Before implementing new features**: Run coverage to see current baseline
2. **While developing**: Use `just coverage-passing` for quick feedback
3. **Before committing**: Ensure new code has reasonable test coverage
4. **For detailed analysis**: Use `just coverage-html` for comprehensive reports

## Files and Directories

- `lcov.info`: LCOV format coverage data (for VS Code)
- `coverage-html/`: HTML coverage reports (when using `just coverage-html`)
- `.gitignore`: Coverage files are excluded from git

Both coverage files are automatically excluded from version control.
