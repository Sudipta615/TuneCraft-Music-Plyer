-- V011: Update app_version to 3.1.3
--
-- V010 last set the app_version to 3.1.0; 3.1.1 and 3.1.2 were patch
-- releases that skipped the migration (check_version_compatibility()
-- silently rewrote the value on each open, but fresh installs momentarily
-- read '3.1.0' between run_migrations() and check_version_compatibility()).
-- This migration brings the stored value in line with the 3.1.3 release.

UPDATE db_metadata SET value = '3.1.3' WHERE key = 'app_version';
