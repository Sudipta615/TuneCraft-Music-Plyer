-- V008: Update app_version from 1.0.0 to 1.0.1
--
-- V007 set the version to 1.0.0 for the initial stable release.
-- This migration updates it to 1.0.1 for the bug-fix release.
-- check_version_compatibility() in repository/mod.rs also
-- auto-updates this value on each open, but this migration ensures
-- fresh installs start with the correct version.

UPDATE db_metadata SET value = '1.0.1' WHERE key = 'app_version';
