-- V009: Update app_version from 1.0.1 to 1.0.2
--
-- V008 set the version to 1.0.1 for the first bug-fix release.
-- This migration updates it to 1.0.2 for the second bug-fix release.
-- check_version_compatibility() in repository/mod.rs also
-- auto-updates this value on each open, but this migration ensures
-- fresh installs start with the correct version.

UPDATE db_metadata SET value = '1.0.2' WHERE key = 'app_version';
