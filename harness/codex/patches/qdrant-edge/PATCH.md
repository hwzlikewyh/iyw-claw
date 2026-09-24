# Qdrant Edge Integration Patch

Source: crates.io `qdrant-edge` 0.8.0, Apache-2.0.
Archive SHA-256: `0b8072302c87506a34bffec9bc16dbdcd36df8ab1321406b6e141530348c7e54`.

The production source and build scripts match the published archive. The
normalized Cargo manifest omits `parking_lot/deadlock_detection`, which cannot
coexist with the `send_guard` feature required by Codex's nucleo and pagable
dependencies. Qdrant production code does not call the deadlock detector API.
The change disables only that diagnostic feature; it retains the embedded
vector index, locking, persistence, and remaining dependency features.

Recheck the conflict on either dependency's next upgrade. Cargo.toml.orig and
the upstream lockfile are preserved as provenance, not used by the application
dependency resolver. The application lockfile controls the combined graph.
