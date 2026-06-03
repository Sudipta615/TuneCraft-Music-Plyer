-- V007: Fix stale app_version in db_metadata
--
-- The stored app_version was '0.8.10' from V005 and was never updated
-- through 15 subsequent releases. This migration updates it to match
-- the current application version. check_version_compatibility() in
-- repository/mod.rs also auto-updates this value on each open, but
-- this migration ensures fresh installs start with the correct version.

UPDATE db_metadata SET value = '1.0.0' WHERE key = 'app_version';
