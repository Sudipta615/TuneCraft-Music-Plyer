-- V010: Update app_version to 3.1.0
--
-- V009 last set the app_version to 1.0.2. The crate version has since
-- advanced to 3.1.0 but no migration was added to track this in the
-- db_metadata table. check_version_compatibility() in repository/mod.rs
-- silently rewrites the value to CARGO_PKG_VERSION on each open, but
-- fresh installs momentarily read '1.0.2' between run_migrations() and
-- check_version_compatibility(). This migration ensures the value is
-- correct from the very first open.

UPDATE db_metadata SET value = '3.1.0' WHERE key = 'app_version';
