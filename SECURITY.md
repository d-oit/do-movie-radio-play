# Security Policy

## Reporting Security Vulnerabilities

If you discover a security vulnerability within `do-movie-radio-play`, please report it responsibly rather than opening a public issue.

Send vulnerability details and reproduction steps to the project security maintainers or open a draft security advisory via GitHub Security Advisories.

---

## Supply Chain Integrity & Release Attestation

Release artifacts published to GitHub Releases undergo automated build provenance attestation, SBOM generation, checksum generation, and fail-closed verification before publication.

### Covered Release Artifacts

The following release assets are generated and attested during automated release workflow execution:

- `movie-radio-timeline-v<VERSION>-x86_64-unknown-linux-gnu.tar.gz`: Pre-compiled release binary archive for Linux `x86_64`.
- `schemas-v<VERSION>.tar.gz`: Consolidated JSON Schema definitions.
- `sbom-v<VERSION>.cdx.json`: CycloneDX v1.4 JSON Software Bill of Materials (SBOM) for the Rust dependency graph derived from resolved locked dependencies (`Cargo.lock`).
- `SHA256SUMS`: Manifest containing SHA-256 cryptographic hashes for all packaged release archives and SBOM files.

> **Target & Platform Scope**: Automated release archives are built for `x86_64-unknown-linux-gnu`. Source builds on other architectures or OS platforms should be compiled locally using `cargo build --release`.

---

## Software Bill of Materials (SBOM)

The SBOM (`sbom-v<VERSION>.cdx.json`) is generated directly from the locked Rust dependency graph (`Cargo.lock`) using `cargo-cyclonedx`.

- **Purpose & Scope**: The SBOM provides dependency inventory and incident-response evidence for security vulnerability scanning and dependency tracking.
- **Security Boundary**: Publishing an SBOM does not prevent malicious upstream dependency publication; it establishes transparency and enables rapid vulnerability identification when CVEs are disclosed in third-party crates.

---

## Consumer Verification Instructions

Consumers downloading release archives from GitHub Releases can verify both artifact integrity and supply chain provenance.

### 1. Verify Cryptographic Checksums

Checksums detect bit rot and file corruption during transit:

```bash
# Download release archives and SHA256SUMS from GitHub Releases
sha256sum -c SHA256SUMS
```

> **Note on Checksums vs. Signed Provenance**: A checksum file alone detects accidental corruption if the reference checksum is trusted. However, it does not prove which workflow, repository, tag, or commit produced the archive. Signed GitHub Artifact Attestations establish cryptographic proof of source commit and workflow identity.

### 2. Verify Signed Build Provenance and SBOM Attestations

Use the GitHub CLI (`gh`) to verify that the downloaded archive was built by this repository's automated release workflow on GitHub Actions:

```bash
# Verify build provenance attestation
gh attestation verify movie-radio-timeline-v1.0.0-x86_64-unknown-linux-gnu.tar.gz \
  --repo d-oit/do-movie-radio-play

# Verify SBOM attestation
gh attestation verify schemas-v1.0.0.tar.gz \
  --repo d-oit/do-movie-radio-play
```

Successful verification confirms that:
1. The archive bytes match the exact build outputs generated during the GitHub Actions run.
2. The release build originated from the authentic source repository (`d-oit/do-movie-radio-play`), trigger ref, and commit SHA.
