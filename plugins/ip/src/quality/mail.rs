//! Outbound TCP port-25 SMTP banner checks across eight major global and regional mail providers.

use std::net::SocketAddr;
use std::time::Duration;

use bench_api::Context;
use bench_api::IpFamily;
use bench_api::NetError;
use bench_api::SmtpBanner;

use super::result::MailCheck;
use super::result::MailStatus;
use super::result::Output;

/// Standard SMTP port.
const SMTP_PORT: u16 = 25;

/// Per-provider TCP + greeting timeout.
const MAIL_TIMEOUT: Duration = Duration::from_secs(5);

/// Static specification of a mail provider's inbound MX server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailTarget {
    /// Stable lowercase identifier.
    pub id: &'static str,
    /// Display name in the report.
    pub name: &'static str,
    /// Primary inbound MX hostname.
    pub mx: &'static str,
}

/// Eight major global and regional email providers checked on port 25.
pub const PROVIDERS: [MailTarget; 8] = [
    MailTarget {
        id: "gmail",
        name: "Gmail",
        mx: "gmail-smtp-in.l.google.com",
    },
    MailTarget {
        id: "outlook",
        name: "Outlook",
        mx: "outlook-com.olc.protection.outlook.com",
    },
    MailTarget {
        id: "yahoo",
        name: "Yahoo",
        mx: "mta5.am0.yahoodns.net",
    },
    MailTarget {
        id: "icloud",
        name: "iCloud",
        mx: "mx01.mail.icloud.com",
    },
    MailTarget {
        id: "proton",
        name: "Proton Mail",
        mx: "mail.protonmail.ch",
    },
    MailTarget {
        id: "fastmail",
        name: "Fastmail",
        mx: "in1-smtp.messagingengine.com",
    },
    MailTarget {
        id: "qq",
        name: "QQ Mail",
        mx: "mx3.qq.com",
    },
    MailTarget {
        id: "netease",
        name: "163 Mail",
        mx: "163mx01.mxmail.netease.com",
    },
];

/// Classifies an [`SmtpBanner`] or [`NetError`] into [`MailStatus`].
#[must_use]
pub fn classify(result: Result<SmtpBanner, NetError>) -> MailStatus {
    match result {
        Ok(SmtpBanner { code }) if (200..300).contains(&code) => MailStatus::Deliverable(code),
        Ok(SmtpBanner { code }) if (400..600).contains(&code) => MailStatus::Rejected(code),
        Ok(SmtpBanner { code }) => MailStatus::Unexpected(format!("SMTP {code}")),
        Err(NetError::NoAddress) => MailStatus::NoAddress,
        Err(NetError::TimedOut | NetError::Transport(_)) => MailStatus::Blocked,
        Err(NetError::NoResolver) => MailStatus::Unexpected("no DNS resolver".to_owned()),
        Err(NetError::Cancelled) => MailStatus::Unexpected("interrupted".to_owned()),
    }
}

/// Probes one [`MailTarget`] over `family` (or `smtp_override` in local smoke tests).
pub async fn check_provider(
    ctx: &Context<Output>,
    family: IpFamily,
    target: MailTarget,
    smtp_override: Option<SocketAddr>,
) -> MailCheck {
    let outcome = match smtp_override {
        Some(addr) => {
            let host = addr.ip().to_string();
            ctx.smtp_banner(family, &host, addr.port(), MAIL_TIMEOUT)
                .await
        }
        None => {
            ctx.smtp_banner(family, target.mx, SMTP_PORT, MAIL_TIMEOUT)
                .await
        }
    };
    MailCheck {
        id: target.id,
        name: target.name,
        status: classify(outcome),
    }
}
