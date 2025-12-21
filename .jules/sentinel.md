## 2024-03-21 - [CRITICAL] Insecure Defaults in SSL Verification
**Vulnerability:** The application disabled SSL verification (`splunk_verify_ssl = false`) by default in the `Config` struct and fell back to `false` when environment variable parsing failed.
**Learning:** Insecure defaults create silent vulnerabilities. Users might assume a security tool is secure by default. A typo in configuration shouldn't result in disabled security.
**Prevention:** Always default boolean security flags to `true` (secure). Use `unwrap_or(true)` when parsing security-related configuration to ensure fail-safe behavior.
