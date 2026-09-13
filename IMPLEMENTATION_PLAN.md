# Implementation Plan: Cross-Platform CI/CD Pipeline & Minimal Distribution Strategy

## 1. Overview & Objectives

Provide an automated CI/CD distribution pipeline allowing anyone on **Windows**, **macOS** (Intel & Apple Silicon), or **Linux** to download a single lightweight executable or release archive and immediately run `sam-fuzzy` with zero setup.

### Core Metrics & Footprint
- **Current Raw Dataset**: `data/sam_media.json` is **93.32 MB** raw JSON.
- **Compressed Dataset**: Gzip level 9 reduces it to **5.12 MB** (~94.5% reduction).
- **Current Release Binary**: **1.5 MB** (stripped x86_64 ELF).
- **Target Distribution Footprint**: Single standalone executable of **~6.6 MB** (or ~5.5 MB release archive).
- **Target Supported Platforms**:
  - `x86_64-unknown-linux-gnu` (glibc Linux)
  - `x86_64-unknown-linux-musl` (Static Linux for Alpine, Void, Nix, and any glibc version)
  - `x86_64-apple-darwin` (macOS Intel)
  - `aarch64-apple-darwin` (macOS Apple Silicon M1/M2/M3/M4)
  - `x86_64-pc-windows-msvc` (Windows 10/11 x64)

---

## 2. Architecture & Data Strategy

```
[ Build & Packaging Time ]
  Raw Dataset (93.3 MB) ──> Gzip/Miniz Compression ──> sam_media.json.gz (5.1 MB)
                                                                │
                                                Embedded via include_bytes!
                                                                │
                                                                ▼
                                                   Standalone Binary (~6.6 MB)

[ Runtime Execution ]
  sam-fuzzy launch
        │
        ├── 1. CLI Override: --data <PATH> (.json or .gz)
        │
        ├── 2. Local File: Check ./data/sam_media.json(.gz)
        │
        └── 3. Embedded Fallback: Stream-decompress embedded 5.1 MB dataset in-memory
```

### Strategy Comparison

| Strategy | Download Size | Disk Footprint | User Experience | Offline Capability |
|---|---|---|---|---|
| **Option 1: Raw JSON in Archive** | ~15 MB archive | ~95 MB extracted | Requires `data/` folder alongside binary | Full |
| **Option 2: Embedded Raw JSON** | ~35 MB binary | ~95 MB binary | Single file, no folder needed | Full |
| **Option 3: Embedded Compressed Dataset (Recommended)** | **~6.6 MB binary** | **~6.6 MB binary** | **Single standalone executable, zero dependencies** | Full |
| **Option 4: Download on First Run** | ~1.5 MB binary | ~95 MB after download | Requires internet and SamOnline connectivity on first launch | Fails without network |

### Recommended Decision: Option 3
- Embed the compressed dataset (`data/sam_media.json.gz`, ~5.1 MB) directly into the binary with transparent streaming decompression at startup (~150ms).
- Still allows local file overrides (via `--data` or local `./data/sam_media.json`) so users can provide updated dataset files whenever desired.
- Produces a **single self-contained executable (~6.6 MB)** that runs immediately on any clean system.

---

## 3. Detailed Implementation Phases

### Phase 1: Dataset Compression & Stream Decompression Engine
1. **Dependency**:
   - Add `flate2 = { version = "1.0", default-features = false, features = ["rust_backend"] }` to `Cargo.toml`.
   - Pure-Rust backend (`miniz_oxide`) eliminates any C/CMake toolchain dependencies, guaranteeing effortless cross-compilation across all target platforms.
2. **Dataset Compression**:
   - Generate `data/sam_media.json.gz` (5.1 MB) using maximum gzip compression.
3. **Transparent Loader in `models.rs` & `main.rs`**:
   - Update `load_dataset_from_reader`:
     - Inspect magic bytes: if starting with `0x1F, 0x8B` (gzip header), stream through `flate2::read::GzDecoder`.
     - Otherwise, stream parse as standard uncompressed JSON.
   - Update `resolve_data_path`:
     - Check local files first (`sam_media.json.gz` and `sam_media.json`).
     - If neither exists on disk and no `--data` flag is passed, load the compiled-in embedded dataset (`include_bytes!("../data/sam_media.json.gz")`).
4. **Test Suite**:
   - Ensure all 27 existing tests pass seamlessly with both compressed streams and raw files.

### Phase 2: GitHub Actions CI Workflow (`.github/workflows/ci.yml`)
- Trigger: Every pull request and push to `main`.
- Runners:
  - `ubuntu-latest`
  - `macos-latest`
  - `windows-latest`
- Tasks:
  - Toolchain setup with `dtolnay/rust-toolchain@stable`.
  - Dependency caching via `swatinem/rust-cache@v2`.
  - Style check: `cargo fmt --check`.
  - Linter check: `cargo clippy --all-targets -- -D warnings`.
  - Full test suite: `cargo test --all-targets --verbose`.

### Phase 3: Automated Cross-Platform Release Workflow (`.github/workflows/release.yml`)
- Trigger: Tag push matching `v*.*.*` (e.g. `v0.1.0`) or manual `workflow_dispatch`.
- Build Matrix:
  1. `x86_64-unknown-linux-gnu` (Linux x86_64)
  2. `x86_64-unknown-linux-musl` (Static Linux, universal compatibility)
  3. `x86_64-apple-darwin` (macOS Intel)
  4. `aarch64-apple-darwin` (macOS Apple Silicon)
  5. `x86_64-pc-windows-msvc` (Windows x64)
- Packaging:
  - Binary strip (`strip` on Unix, compiler profile on Windows).
  - Package binary + `README.md` + `LICENSE` into:
    - Linux/macOS: `sam-fuzzy-v{version}-{target}.tar.gz`
    - Windows: `sam-fuzzy-v{version}-{target}.zip`
  - Generate SHA-256 checksums (`sam-fuzzy-v{version}-checksums.sha256`).
- GitHub Release:
  - Automatically create release with changelog via `softprops/action-gh-release@v2`.
  - Attach all 5 platform packages and SHA-256 checksums.

### Phase 4: Automated Dataset Maintenance Workflow (`.github/workflows/update-dataset.yml`)
- Trigger: Scheduled monthly cron (`0 0 1 * *`) or manual `workflow_dispatch`.
- Execution:
  1. Executes `scripts/fetch_file_sizes.py` to index new additions on DhakaFlix.
  2. Recompresses dataset to `data/sam_media.json.gz`.
  3. Runs test suite to ensure schema integrity.
  4. Automatically opens a Pull Request with updated counts and changes.

### Phase 5: Installation & Quick-Start Documentation
- Add installation guide to `README.md`:
  - Release download links and platform table.
  - One-line install command for Linux and macOS:
    ```bash
    curl -fsSL https://raw.githubusercontent.com/Shantodotdev/sam-fuzzy/main/install.sh | bash
    ```
  - Windows PowerShell install instructions.

---

## 4. Verification & Testing

1. **Standalone Execution Test**:
   - Move compiled release binary to a clean temporary directory with no `data/` folder.
   - Run `sam-fuzzy` and verify that all 104,650 items load and index properly in <1s.
2. **Override Test**:
   - Provide a sample 2-item dataset via `--data` and verify that runtime override takes precedence.
3. **Workflow Validation**:
   - Validate GitHub Actions YAML syntax and schema.
   - Test release draft build and artifact creation.
