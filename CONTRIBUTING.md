# Contributing to Splunk TUI

Thank you for your interest in contributing to Splunk TUI! We welcome contributions from everyone.

## Getting Started

1.  **Fork the repository** on GitHub.
2.  **Clone your fork** locally:
    ```bash
    git clone https://github.com/YOUR_USERNAME/splunk-tui.git
    cd splunk-tui
    ```
3.  **Install dependencies**:
    Ensure you have [Rust and Cargo](https://rustup.rs/) installed.
4.  **Create a branch** for your feature or bug fix:
    ```bash
    git checkout -b my-new-feature
    ```

## Development Workflow

### Building and Running
To build and run the project locally:
```bash
cargo run
```

### Testing
We encourage writing tests for new features. Run existing tests with:
```bash
cargo test
```

### Code Style
We follow standard Rust formatting. Please run `cargo fmt` before submitting a pull request:
```bash
cargo fmt --all
```

### Linting
We use `clippy` for linting. Please ensure your changes pass clippy checks:
```bash
cargo clippy -- -D warnings
```

## Submitting Changes

1.  **Commit your changes** with clear and descriptive commit messages.
2.  **Push to your fork**:
    ```bash
    git push origin my-new-feature
    ```
3.  **Open a Pull Request** against the `main` branch of the original repository.

## Reporting Issues

If you find a bug or have a feature request, please [open an issue](https://github.com/PicoMitchell/splunk-tui/issues) on GitHub. Provide as much detail as possible, including steps to reproduce the issue.

## License

By contributing to Splunk TUI, you agree that your contributions will be licensed under the project's [MIT License](LICENSE).
