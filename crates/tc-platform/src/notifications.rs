//! Desktop notification dispatch
//!
//! Contains platform-specific notification logic and the
//! [`PlatformIntegration`] notification methods. Notifications are
//! dispatched asynchronously to avoid blocking the UI thread.

use crate::types::PlatformError;
use crate::PlatformIntegration;

impl PlatformIntegration {
    /// Send a desktop notification.
    ///
    ///
    /// detached thread to execute the platform-specific notification command,
    /// so the UI thread is never blocked by slow notification daemons (S1).
    pub fn send_notification(&self, title: &str, body: &str) -> Result<(), PlatformError> {
        log::info!("Notification: {} - {}", title, body);
        let title = title.to_string();
        let body = body.to_string();
        std::thread::spawn(move || {
            if let Err(e) = dispatch_notification_sync(&title, &body) {
                log::warn!("Notification dispatch failed: {}", e);
            }
        });
        Ok(())
    }
}

/// Dispatch a desktop notification using platform-specific commands.
/// This runs synchronously and should be called from a background thread.
pub(crate) fn dispatch_notification_sync(title: &str, body: &str) -> Result<(), PlatformError> {
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        // arguments directly to the process without shell interpretation,
        // so escaping is unnecessary and causes literal backslashes to
        // appear in notifications.
        let status = Command::new("notify-send").arg(title).arg(body).status();
        match status {
            Ok(s) if s.success() => return Ok(()),
            Ok(s) => {
                return Err(PlatformError::Other(format!(
                    "notify-send exited with status: {}",
                    s
                )))
            },
            Err(e) => {
                return Err(PlatformError::Other(format!(
                    "Failed to run notify-send: {}",
                    e
                )))
            },
        }
    }

    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};

        // Use osascript's stdin instead of -e with an interpolated string.
        // This completely avoids AppleScript injection: the title and body
        // are never embedded inside the script source; instead the script
        // reads them from environment variables (which osascript exposes as
        // AppleScript globals when set before the process starts).
        // Any characters — including quotes, backslashes, newlines, emoji —
        // are passed verbatim through the environment without escaping.
        let script = "set envTitle to system attribute \"NOTIFICATION_TITLE\"\n\
                      set envBody to system attribute \"NOTIFICATION_BODY\"\n\
                      display notification envBody with title envTitle\n";

        let mut child = Command::new("osascript")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env("NOTIFICATION_TITLE", title)
            .env("NOTIFICATION_BODY", body)
            .spawn()
            .map_err(|e| PlatformError::Other(format!("Failed to spawn osascript: {}", e)))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(script.as_bytes()).map_err(|e| {
                PlatformError::Other(format!("Failed to write osascript stdin: {}", e))
            })?;
        }

        let status = child
            .wait()
            .map_err(|e| PlatformError::Other(format!("osascript wait failed: {}", e)))?;

        if status.success() {
            return Ok(());
        }
        return Err(PlatformError::Other(format!(
            "osascript exited with status: {}",
            status
        )));
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let ps_script = "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
                         [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null; \
                         $template = '<toast><visual><binding template=\"ToastText02\"><text id=\"1\"></text><text id=\"2\"></text></binding></visual></toast>'; \
                         $xml = New-Object Windows.Data.Xml.Dom.XmlDocument; \
                         $xml.LoadXml($template); \
                         $nodeList = $xml.GetElementsByTagName('text'); \
                         if ($nodeList.Count -ge 2) { \
                             $nodeList.Item(0).InnerText = $env:TOAST_TITLE; \
                             $nodeList.Item(1).InnerText = $env:TOAST_BODY; \
                         } \
                         $toast = [Windows.UI.Notifications.ToastNotification]::new($xml); \
                         [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('TuneCraft').Show($toast)";
        let status = Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(ps_script)
            .env("TOAST_TITLE", title)
            .env("TOAST_BODY", body)
            .status();
        match status {
            Ok(s) if s.success() => return Ok(()),
            Ok(s) => {
                return Err(PlatformError::Other(format!(
                    "PowerShell toast exited with status: {}",
                    s
                )))
            },
            Err(e) => {
                return Err(PlatformError::Other(format!(
                    "Failed to run PowerShell: {}",
                    e
                )))
            },
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (title, body);
        Err(PlatformError::NotAvailable(
            "Desktop notifications not supported on this platform".to_string(),
        ))
    }
}

/// Escape a string for use inside AppleScript double-quoted strings.
/// AppleScript string literals use backslash as the escape character for
/// double quotes and backslashes.

/// Also escapes dangerous AppleScript metacharacters including line breaks, to prevent
/// injection attacks when notification text comes from untrusted sources
/// (e.g., track titles from maliciously-crafted music files).
pub fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Escape a string for safe embedding in XML content (e.g., Windows toast templates).
/// Escapes the five XML-special characters.
pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
