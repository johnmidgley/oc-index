# Improvements

Tracking list of bugs and missing tests found during code review. Check items off as they're fixed.

## Bugs

- [x] **`.oci` prefix check is too broad** — `src/commands.rs:273`, `src/commands.rs:572`, `src/commands.rs:1069`, `src/commands.rs:1469`
  `rel_str.starts_with(".oci")` also matches `.ocirc`, `.ocissues`, `.ocignore-backup`, etc., silently excluding them from scanning/updating/pruning. Should be `rel_str == ".oci" || rel_str.starts_with(".oci/")`.
  Fixed at all 4 sites; regression test `test_dot_oci_prefix_does_not_match_unrelated_dotfiles` added.

- [x] **`Config::load` silently masks corruption** — `src/config.rs:42-65`
  If the file exists but has no `version=` line, `version` stays at `TOOL_VERSION` and `check_version()` returns true. A truncated or garbage config never warns the user — version-mismatch detection silently fails.
  Fixed: `version` is now `Option<String>` and `load()` errors out with a clear message if no `version=` line was parsed. Regression test `test_config_missing_version_field_errors` added.

- [x] **`prune --restore` can overwrite user files** — `src/commands.rs:944`
  `fs::rename(entry.path(), &original_path)` overwrites the destination on Linux without warning. If the user created a file at the original path after pruning, restore silently destroys it. Check destination existence first.
  Fixed: conflicting destinations are now skipped with a stderr warning and left in `.oci/pruneyard/`. Pruneyard is only removed when restore is complete (zero conflicts). Regression test `test_prune_restore_skips_existing_destination` added.

- [x] **`init` leaves a half-baked `.oci` on failure** — `src/commands.rs:87-98`
  `fs::create_dir_all(&oci_dir)` runs before `Index::new()`, `save()`, `Config::new().save()`, and `init_ignore_file()`. If any of those fail, the empty `.oci` is left behind and the next `init` aborts with "Index already exists", masking the real cause and forcing manual `rm -rf .oci`.
  Fixed: inner steps moved into `populate_new_index`; on failure the partial `.oci` is removed before the error is returned. No automated regression test — the early `if oci_dir.exists() { bail }` check makes it awkward to set up a half-baked state from an integration test without restructuring further. Verified by inspection.

- [x] **`ScanResult::ignored_files` is dead** — `src/scanner.rs:15`, `src/scanner.rs:37`
  Declared but never inserted into; always returned empty. `#[allow(dead_code)]` is a hint it was forgotten. Either remove the field or actually populate it — right now it looks live but isn't.
  Fixed: removed the dead field. `WalkDir::filter_entry` prunes ignored paths before they're visited, so populating it would require changing the walk; defer until verbose `I` markers actually need it.

- [x] **`ignore::add_pattern` doesn't deduplicate** — `src/ignore.rs:58-83`
  Running `oci ignore *.log` twice appends `*.log` twice. The `ignore` file grows unbounded.
  Fixed: `add_pattern` now scans existing trimmed lines and returns `Ok(false)` if the pattern is already present; the caller prints "Pattern already in ignore" instead of "Added". Regression test `test_ignore_add_dedupes` added.

- [x] **Inconsistent byte formatting**
  `format_bytes()` exists (`src/commands.rs:51-65`) and is used by `hogs` and `prune`, but `duplicates` (`src/commands.rs:892`) and `stats` (`src/commands.rs:1426`, `src/commands.rs:1433`) hardcode `bytes as f64 / 1_048_576.0` and always print MB. A 100-byte duplicate set shows "0.00 MB"; a 10 GB total shows "10240.00 MB" instead of "10.00 GB".
  Fixed: `duplicates` and `stats` now use `format_bytes()` like the other call sites. Format changed from `(X.XX MB)` to `(X.XX KB|MB|GB|bytes)` — no tests asserted on the old MB-only string.

- [x] **`find_files_to_prune` reason is sticky** — `src/commands.rs:1027-1058`
  Reason is set to "duplicate" first, but a later ignore-pattern match overwrites it to "ignored". A file that's both a duplicate AND ignored is reported only as "ignored" and the duplicate counter is missed.
  Fixed: the two ignore-pattern branches now only set `prune_reason` if it is still empty, so an earlier "duplicate" classification wins. Regression test `test_prune_reason_duplicate_wins_over_ignored` added.

- [x] **Single-file `update` errors instead of detecting deletion** — `src/commands.rs:733`
  If a file in the index is removed from disk and you run `oci update path/to/file.txt`, you get `Path does not exist` and the index isn't updated. The dir-update path correctly handles deletion; the single-file path should align.
  Fixed: before bailing on a missing path, `update` now checks if the resolved rel-path is in the index; if so it emits a `-` marker, removes the entry, and reports `1 removed`. Regression test `test_update_single_file_detects_deletion` added.

## Missing tests

User-visible behaviors with no integration test:

- [x] **Version-mismatch warning** — `Config::warn_version_mismatch` has zero coverage. Hand-write a `.oci/config` with `version=0.0.1`, run any command, assert on stderr.
  Added `test_version_mismatch_warning`: writes `version=0.0.1`, runs `oci ls`, asserts on the warning text and that the command still succeeds.
- [x] **`oci ignore` with no argument** — `src/commands.rs:120-125` (uses current directory). No test.
  Added `test_ignore_no_argument_uses_current_directory`: runs `oci ignore` with no args from a subdir and asserts the subdir's relative path is added to the ignore file.
- [x] **`ocignore` → `ignore` migration** — `src/ignore.rs:22-31`. No test.
  Added `test_ocignore_to_ignore_migration`: replaces the modern `.oci/ignore` with an old-style `.oci/ocignore`, runs `update`, and asserts the file was renamed and the legacy pattern is honored.
- [x] **`-v` verbose** for both `status` and `update` (`=` and `I` markers). README documents the output; tests never assert on it.
  Added `test_status_verbose_shows_unchanged_and_ignored_markers` and `test_update_verbose_shows_unchanged_and_ignored_markers`: assert that `=` (unchanged) and `I` (ignored) marker lines appear only with `-v`.
- [x] **`oci grep` with no match** — `src/commands.rs:824-827` ("No files found with hash:"). Untested.
  Added `test_grep_with_no_match`: greps for an all-zeros hash and asserts the "No files found" message with a 0 exit code.
- [x] **Single-file `oci update file.txt`** — `update_single_file` exists but every integration test calls `update` on a directory.
  Added `test_update_single_file_add_and_modify`: asserts `+` marker on first add, `U` on subsequent modification, and that a sibling file is NOT touched by a single-file update. (Deletion path covered by `test_update_single_file_detects_deletion`.)
- [x] **`reset` / `deinit` confirmation flow** — only the `-f` path is tested. The interactive y/n branch and the "cancelled" output are untested.
  Added `run_oci_with_stdin` helper plus four tests covering both yes/no branches of both commands.
- [x] **Regression test for the `.oci` prefix bug** — added `test_dot_oci_prefix_does_not_match_unrelated_dotfiles`.
- [x] **No unit tests** in `dir_utils.rs`, `display.rs`, or `scanner.rs`.
  Added module-level tests for each: empty-parent cleanup, the `.oci`-prefix skip (also fixed the same prefix bug at `src/dir_utils.rs:61`), `DisplayContext::make_relative` across cwd positions, `StatusMarker` symbols, and `FileScanner` pattern filtering / directory pruning.
- [x] **No unit tests** in `commands.rs` — everything is exercised through the binary.
  Added `test_format_bytes_picks_correct_unit` covering bytes/KB/MB/GB boundary points (the only pure helper worth isolating; the rest of `commands.rs` touches the filesystem or index and is already exercised by integration tests).
- [x] **Empty-file hashing** — every test writes non-empty content. Cover the zero-byte path through `compute_sha256`.
  Added `test_compute_sha256_empty_file` in `src/file_utils.rs` asserting the known SHA256 of the empty string.

## Style nits

- [x] **`static mut OCI_BIN` with `unsafe`** — `tests/integration_tests.rs:7-31`. `OnceLock<PathBuf>` does the same thing safely and is a one-line replacement.
  Fixed: replaced `Once` + `static mut Option<PathBuf>` with `OnceLock<PathBuf>` and `get_or_init`. No more `unsafe` block in the test harness; the `rust_2024_compatibility` warning is gone.
- [x] **Duplicated "permission denied" detection block** — `src/commands.rs:478-490`, `src/commands.rs:510-525`, `src/commands.rs:627-642`, `src/commands.rs:663-677`. Worth a helper.
  Fixed: extracted `fn is_permission_denied(&anyhow::Error) -> bool` and replaced all four inline detection blocks with single-line calls.
