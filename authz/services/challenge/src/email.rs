//! Email through an SMTP relay the client provides (ADR 0122): to people at the address their IdP
//! provisioned, and to the security team's mailbox. Plain text, the same words as the Slack
//! message, so nothing is said by email that Slack wouldn't say.

use std::time::Duration;

use anyhow::{bail, Context};
use lettre::{
    message::{header::ContentType, Mailbox},
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

const TIMEOUT: Duration = Duration::from_secs(10);

pub struct Email {
    relay: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    /// The security team's mailbox or list.
    security: Mailbox,
}

impl Email {
    /// `relay` is an SMTP URL with any credentials in it: `smtps://user:password@host:465` for
    /// TLS from the start, or `smtp://user:password@host:587?tls=required` for STARTTLS. Without
    /// TLS the credentials and messages would cross the network in the clear, so plain SMTP is
    /// refused except to a relay on the same host.
    pub fn new(relay: &str, from: &str, security: &str) -> anyhow::Result<Self> {
        let encrypted = relay.starts_with("smtps://") || relay.contains("tls=required");
        if !encrypted && !["localhost", "127.0.0.1"].contains(&host_of(relay)) {
            bail!("the SMTP relay needs TLS: an smtps:// URL, or smtp:// with ?tls=required");
        }
        let relay = AsyncSmtpTransport::<Tokio1Executor>::from_url(relay)
            .context("the SMTP relay isn't a valid smtp:// or smtps:// URL")?
            .timeout(Some(TIMEOUT))
            .build();
        Ok(Self {
            relay,
            from: from.parse().context("the email sender isn't an email address")?,
            security: security.parse().context("the security team's email isn't an email address")?,
        })
    }

    /// An email to the person at `address`, the email their IdP provisioned or their `userName`.
    pub async fn to_human(&self, address: &str, subject: &str, text: &str) -> anyhow::Result<()> {
        let Ok(to) = address.parse() else {
            bail!("{address} isn't an email address, so they can't be emailed");
        };
        self.send(to, subject, text).await
    }

    /// An email to the security team.
    pub async fn to_security(&self, subject: &str, text: &str) -> anyhow::Result<()> {
        self.send(self.security.clone(), subject, text).await
    }

    async fn send(&self, to: Mailbox, subject: &str, text: &str) -> anyhow::Result<()> {
        let message = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(text.to_string())?;
        self.relay.send(message).await.context("the SMTP relay refused the message")?;
        Ok(())
    }
}

/// The host in an SMTP URL, without credentials, port, or path.
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    host.split(':').next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
    };

    use super::*;

    /// One message a fake relay took: its recipients and its text, headers and all.
    #[derive(Debug, Default, Clone)]
    struct Taken {
        to: Vec<String>,
        data: String,
    }

    /// An SMTP relay on the loopback address that accepts everything and keeps what it took.
    async fn fake_relay() -> (String, Arc<Mutex<Vec<Taken>>>) {
        let taken: Arc<Mutex<Vec<Taken>>> = Arc::default();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let kept = taken.clone();
        tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let kept = kept.clone();
                tokio::spawn(async move {
                    let (read, mut write) = socket.into_split();
                    let mut lines = BufReader::new(read).lines();
                    write.write_all(b"220 fake relay\r\n").await.unwrap();
                    let mut message = Taken::default();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let command = line.to_ascii_uppercase();
                        let reply: &[u8] = if command.starts_with("RCPT TO:") {
                            message.to.push(line[8..].trim().to_string());
                            b"250 ok\r\n"
                        } else if command == "DATA" {
                            write.write_all(b"354 go ahead\r\n").await.unwrap();
                            while let Ok(Some(line)) = lines.next_line().await {
                                if line == "." {
                                    break;
                                }
                                message.data.push_str(&line);
                                message.data.push('\n');
                            }
                            kept.lock().unwrap().push(std::mem::take(&mut message));
                            b"250 taken\r\n"
                        } else if command == "QUIT" {
                            write.write_all(b"221 bye\r\n").await.unwrap();
                            return;
                        } else {
                            b"250 ok\r\n"
                        };
                        write.write_all(reply).await.unwrap();
                    }
                });
            }
        });
        (format!("smtp://127.0.0.1:{}", addr.port()), taken)
    }

    #[tokio::test]
    async fn people_and_the_security_team_get_plain_text_email() {
        let (relay, taken) = fake_relay().await;
        let email = Email::new(&relay, "awzy <awzy@larkspur.example>", "security@larkspur.example").unwrap();
        email
            .to_human("nora@larkspur.example", "awzy needs a reason", "awzy needs a reason. Answer it: https://p/challenges")
            .await
            .unwrap();
        email.to_security("awzy asks for an approval", "Review it: https://p/challenges").await.unwrap();
        assert!(email.to_human("nora", "s", "t").await.is_err(), "not an email address");

        let taken = taken.lock().unwrap();
        assert_eq!(taken.len(), 2);
        assert_eq!(taken[0].to, vec!["<nora@larkspur.example>"]);
        assert!(taken[0].data.contains("Subject: awzy needs a reason"), "{}", taken[0].data);
        assert!(taken[0].data.contains("Content-Type: text/plain"), "{}", taken[0].data);
        assert!(taken[0].data.contains("Answer it: https://p/challenges"), "{}", taken[0].data);
        assert_eq!(taken[1].to, vec!["<security@larkspur.example>"]);
    }

    #[test]
    fn plain_smtp_is_refused_except_to_a_relay_on_the_same_host() {
        let new = |relay| Email::new(relay, "awzy@larkspur.example", "security@larkspur.example");
        assert!(new("smtp://user:pw@relay.larkspur.example:25").is_err());
        assert!(new("smtps://user:pw@relay.larkspur.example:465").is_ok());
        assert!(new("smtp://user:pw@relay.larkspur.example:587?tls=required").is_ok());
        assert!(new("smtp://127.0.0.1:2525").is_ok());
        assert!(new("smtp://localhost").is_ok());
    }
}
