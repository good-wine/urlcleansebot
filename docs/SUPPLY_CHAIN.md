# Supply Chain Security Documentation

This document describes the SLSA Level 2 supply chain security implementation for URLCleanseBot and how to verify artifacts.

## Overview

URLCleanseBot implements **SLSA Level 2** (Supply Chain Levels for Software Artifacts) with the following guarantees:

| Level | Requirement | Status |
|-------|-------------|--------|
| **Build** | Automated build with CI/CD | ✅ GitHub Actions |
| **Provenance** | Tamper-resistant build provenance | ✅ SLSA Provenance via `slsa-github-generator` |
| **Source** | Version-controlled source with history | ✅ GitHub (signed commits) |
| **Dependencies** | Pinned, verified dependencies | ✅ `Cargo.lock`, `cargo-deny`, `cargo-audit` |
| **Signing** | Cryptographic signatures on artifacts | ✅ Cosign keyless (OIDC) |
| **SBOM** | Software Bill of Materials | ✅ Syft (SPDX + CycloneDX) |

## Artifacts Produced

Every release (`v*` tag) produces the following signed artifacts:

| Artifact | Description | Formats |
|----------|-------------|---------|
| **Binary** | Release binary (`url_cleanse_bot`) | ELF (Linux x86_64) |
| **Container Image** | Multi-arch Docker image | `linux/amd64`, `linux/arm64` |
| **Source SBOM** | SBOM from source code | SPDX JSON, CycloneDX JSON |
| **Container SBOM** | SBOM from container image | SPDX JSON, CycloneDX JSON |
| **Provenance** | SLSA Provenance attestation | in-toto JSONL |
| **Checksums** | SHA256 of all artifacts | `checksums.txt` |

All artifacts are signed with **Cosign keyless signing** using GitHub Actions OIDC.

## Verification Guide

### Prerequisites

```bash
# Install cosign
# macOS: brew install cosign
# Linux: curl -O -L "https://github.com/sigstore/cosign/releases/latest/download/cosign-linux-amd64" && sudo install cosign-linux-amd64 /usr/local/bin/cosign

# Install syft (for SBOM inspection)
# curl -sSfL https://raw.githubusercontent.com/anchore/syft/main/install.sh | sh -s -- -b /usr/local/bin

# Install gh CLI (for downloading releases)
# https://cli.github.com/
```

### 1. Verify Container Image

```bash
# Set version
VERSION=v0.2.0

# Verify image signature
cosign verify \
  --certificate-identity-regexp "https://github.com/good-wine/urlcleansebot/.github/workflows/.*" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  ghcr.io/good-wine/urlcleansebot@${VERSION}

# Verify SBOM attestations
cosign verify-attestation \
  --type spdxjson \
  --certificate-identity-regexp "https://github.com/good-wine/urlcleansebot/.github/workflows/.*" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  ghcr.io/good-wine/urlcleansebot@${VERSION}

cosign verify-attestation \
  --type cyclonedxjson \
  --certificate-identity-regexp "https://github.com/good-wine/urlcleansebot/.github/workflows/.*" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  ghcr.io/good-wine/urlcleansebot@${VERSION}

# Verify SLSA Provenance
cosign verify-attestation \
  --type slsaprovenance \
  --certificate-identity-regexp "https://github.com/good-wine/urlcleansebot/.github/workflows/.*" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  ghcr.io/good-wine/urlcleansebot@${VERSION}
```

### 2. Verify Release Artifacts (from GitHub Releases)

```bash
VERSION=v0.2.0
REPO=good-wine/urlcleansebot

# Download all release assets
gh release download ${VERSION} --repo ${REPO} --dir ./release-${VERSION}

cd ./release-${VERSION}

# Verify binary
cosign verify-blob \
  --certificate url_cleanse_bot.pem \
  --signature url_cleanse_bot.sig \
  --certificate-identity-regexp "https://github.com/good-wine/urlcleansebot/.github/workflows/.*" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  url_cleanse_bot

# Verify checksums file
cosign verify-blob \
  --certificate checksums.txt.pem \
  --signature checksums.txt.sig \
  --certificate-identity-regexp "https://github.com/good-wine/urlcleansebot/.github/workflows/.*" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  checksums.txt

# Verify checksums match
sha256sum -c checksums.txt

# Verify SBOMs
for fmt in spdx cyclonedx; do
  cosign verify-blob \
    --certificate sbom.${fmt}.json.pem \
    --signature sbom.${fmt}.json.sig \
    --certificate-identity-regexp "https://github.com/good-wine/urlcleansebot/.github/workflows/.*" \
    --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
    sbom.${fmt}.json
done

# Verify container SBOMs
for fmt in spdx cyclonedx; do
  cosign verify-blob \
    --certificate sbom.container.${fmt}.json.pem \
    --signature sbom.container.${fmt}.json.sig \
    --certificate-identity-regexp "https://github.com/good-wine/urlcleansebot/.github/workflows/.*" \
    --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
    sbom.container.${fmt}.json
done

# Verify provenance
cosign verify-blob \
  --certificate provenance.intoto.jsonl.pem \
  --signature provenance.intoto.jsonl.sig \
  --certificate-identity-regexp "https://github.com/good-wine/urlcleansebot/.github/workflows/.*" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  provenance.intoto.jsonl
```

### 3. Inspect SBOMs

```bash
# View SPDX SBOM summary
syft sbom.spdx.json

# View CycloneDX SBOM summary
syft sbom.cyclonedx.json

# Query for specific packages
syft sbom.spdx.json -o table | grep -E "(tokio|sqlx|teloxide|reqwest)"

# Export to different formats
syft sbom.spdx.json -o cyclonedx-json > converted.cyclonedx.json
```

### 4. Inspect Provenance

```bash
# View provenance (in-toto format)
cat provenance.intoto.jsonl | jq .

# Key fields to verify:
# - predicate.buildType: "https://github.com/slsa-framework/slsa-github-generator/github-actions@v1"
# - predicate.builder.id: "https://github.com/good-wine/urlcleansebot/.github/workflows/supply-chain.yml@..."
# - predicate.invocation.configSource.uri: "https://github.com/good-wine/urlcleansebot"
# - predicate.materials: [] (source repo, dependencies)
```

### 5. Verify Rekor Transparency Log Entry

```bash
# Verify the signature was logged to Rekor
rekor-cli search --sha256 <sha256-of-artifact>

# Or use cosign's built-in verification (includes Rekor check)
cosign verify-blob --rekor-url https://rekor.sigstore.dev ...
```

## CI/CD Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      supply-chain.yml                           │
├─────────────────────────────────────────────────────────────────┤
│  source-sbom          │  build-and-provenance  │  cosign-sign   │
│  ├─ Syft (source)     │  ├─ Docker buildx      │  ├─ Sign image │
│  │  ├─ SPDX           │  │  ├─ Multi-arch       │  ├─ Attest   │
│  │  └─ CycloneDX      │  │  └─ Push GHCR        │  │  SBOM      │
│  └─ Upload artifacts  │  ├─ SLSA Provenance    │  ├─ Attest   │
│                       │  │  └─ slsa-generator   │  │  Provenance│
│                       │  ├─ Container SBOM     │  ├─ Sign      │
│                       │  │  └─ Syft (image)     │  │  Binary    │
│                       │  └─ Upload artifacts   │  ├─ Sign      │
│                       │                        │  │  SBOMs     │
│                       │                        │  └─ Upload    │
├─────────────────────────────────────────────────────────────────┤
│                        verify                                    │
│  ├─ Verify image signature                                       │
│  ├─ Verify SBOM attestations                                     │
│  ├─ Verify provenance attestation                                │
│  └─ Verify blob signatures                                       │
├─────────────────────────────────────────────────────────────────┤
│                    dependency-audit                              │
│  ├─ cargo-audit (vulnerabilities)                                │
│  └─ cargo-deny (licenses, bans, sources)                        │
├─────────────────────────────────────────────────────────────────┤
│                    release-assembly (on tag)                    │
│  ├─ Download signed artifacts                                    │
│  ├─ Generate checksums.txt                                       │
│  ├─ Sign checksums                                               │
│  ├─ Generate changelog (git-cliff)                               │
│  └─ Create GitHub Release with all artifacts                    │
└─────────────────────────────────────────────────────────────────┘
```

## Security Properties

### What This Protects Against

| Threat | Mitigation |
|--------|------------|
| **Build tampering** | SLSA provenance proves build ran in GitHub Actions with expected source |
| **Dependency substitution** | `Cargo.lock` pinned, `cargo-deny` enforces registry sources, SBOMs show exact versions |
| **Artifact tampering post-build** | Cosign signatures on all artifacts + Rekor transparency log |
| **Key compromise** | Keyless signing - no long-lived private keys exist |
| **Typosquatting** | `cargo-deny` blocks unknown registries/git sources |
| **License violations** | `cargo-deny` enforces allow-list |

### Trust Root

The trust root is **GitHub's OIDC provider** (`https://token.actions.githubusercontent.com`). Verification checks that:

1. The certificate was issued by GitHub's OIDC provider
2. The certificate identity matches the expected workflow: `https://github.com/good-wine/urlcleansebot/.github/workflows/*`
3. The signature validates against the certificate's public key
4. The certificate is logged in Rekor (public transparency log)

## Upgrading to SLSA Level 3

To reach SLSA Level 3, the following would be needed:

- [ ] **Ephemeral build environments**: Build runs in isolated, single-use VMs (GitHub Actions hosted runners are shared)
- [ ] **Non-falsifiable provenance**: Provenance generated by a dedicated, audited build service (not self-generated)
- [ ] **Hardened build platform**: Build service with strict access controls, audit logging, and no persistent state

Current implementation achieves **Level 2** with provenance generated by `slsa-github-generator` which runs in the same workflow but provides tamper-resistant provenance format.

## Troubleshooting

### "Certificate identity doesn't match"

Ensure you're using the correct identity regex:
```
https://github.com/good-wine/urlcleansebot/.github/workflows/.*
```

### "Rekor entry not found"

The signature may not have been logged yet (can take a few minutes). Or the artifact was signed before Rekor integration was enabled.

```bash
# Check Rekor manually
rekor-cli search --sha256 $(sha256sum url_cleanse_bot | cut -d' ' -f1)
```

### "Verification failed for container image"

Ensure you're using the **digest** not the tag:
```bash
# Get digest
docker pull ghcr.io/good-wine/urlcleansebot:v0.2.0
docker inspect ghcr.io/good-wine/urlcleansebot:v0.2.0 --format='{{index .RepoDigests 0}}'

# Verify with digest
cosign verify ghcr.io/good-wine/urlcleansebot@sha256:...
```

### "cargo-audit / cargo-deny failures in CI"

- **Vulnerability found**: Update the affected dependency in `Cargo.toml`
- **License not allowed**: Add to `deny.toml` allow-list with justification, or find alternative
- **Banned crate**: Remove dependency or use allowed alternative

## References

- [SLSA Specification](https://slsa.dev/spec/v1.0)
- [SLSA GitHub Generator](https://github.com/slsa-framework/slsa-github-generator)
- [Cosign Keyless Signing](https://docs.sigstore.dev/cosign/overview/)
- [Syft SBOM](https://github.com/anchore/syft)
- [in-toto Attestation Format](https://github.com/in-toto/attestation)
- [Rekor Transparency Log](https://github.com/sigstore/rekor)