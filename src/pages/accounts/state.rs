//! What one account card shows: the saved account and where a sign-in is.

use crownos_accounts::{Account, AccountError, Client, Provider, sign_out};
use tokio::task::JoinError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Idle,
    WaitingForBrowser,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountCard {
    pub provider: Provider,
    pub account: Account,
    pub phase: Phase,
}

/// `error` and every cause under it, on one line.
pub fn describe(error: impl std::error::Error + Send + Sync + 'static) -> String {
    format!("{:#}", anyhow::Error::new(error))
}

impl AccountCard {
    pub fn load(provider: Provider) -> Self {
        let (account, phase) = match Account::load(provider) {
            Ok(account) => (account, Phase::Idle),
            Err(error) => (Account::default(), Phase::Failed(describe(error))),
        };
        Self {
            provider,
            account,
            phase,
        }
    }

    pub fn is_waiting(&self) -> bool {
        self.phase == Phase::WaitingForBrowser
    }

    pub fn can_sign_in(&self) -> bool {
        !self.is_waiting() && !self.account.client.id.trim().is_empty()
    }

    pub fn status_line(&self) -> String {
        match (&self.phase, &self.account.session) {
            (Phase::WaitingForBrowser, _) => "Waiting for the browser…".to_owned(),
            (_, Some(session)) if session.name.is_empty() => "Connected".to_owned(),
            (_, Some(session)) => format!("Connected as {}", session.name),
            (_, None) => "Not connected".to_owned(),
        }
    }

    pub fn error(&self) -> Option<String> {
        match &self.phase {
            Phase::Failed(message) => Some(message.clone()),
            _ => None,
        }
    }

    pub fn fail(&mut self, message: String) {
        self.phase = Phase::Failed(message);
    }

    /// Changes the OAuth client and saves it next to the session it already has.
    pub fn edit_client(&mut self, edit: impl FnOnce(&mut Client)) {
        let before = self.account.client.clone();
        edit(&mut self.account.client);
        if self.account.client == before {
            return;
        }
        self.phase = match self.account.save(self.provider) {
            Ok(()) => Phase::Idle,
            Err(error) => Phase::Failed(describe(error)),
        };
    }

    pub fn finish_sign_in(&mut self, outcome: Result<Result<Account, AccountError>, JoinError>) {
        match outcome {
            Ok(Ok(account)) => {
                self.account = account;
                self.phase = Phase::Idle;
            }
            Ok(Err(error)) => self.fail(describe(error)),
            Err(error) => self.fail(describe(error)),
        }
    }

    pub fn sign_out(&mut self) {
        self.phase = match sign_out(self.provider) {
            Ok(()) => {
                self.account.session = None;
                Phase::Idle
            }
            Err(error) => Phase::Failed(describe(error)),
        };
    }
}

#[cfg(test)]
mod tests {
    use crownos_accounts::Session;

    use super::*;

    fn card(session: Option<Session>, phase: Phase) -> AccountCard {
        AccountCard {
            provider: Provider::Spotify,
            account: Account {
                client: Client {
                    id: "client".to_owned(),
                    secret: String::new(),
                },
                session,
            },
            phase,
        }
    }

    fn session(name: &str) -> Session {
        Session {
            access_token: "a".to_owned(),
            refresh_token: "r".to_owned(),
            expires_at: 0,
            name: name.to_owned(),
        }
    }

    #[test]
    fn the_status_line_says_who_is_connected() {
        assert_eq!(card(None, Phase::Idle).status_line(), "Not connected");
        assert_eq!(
            card(Some(session("Ada")), Phase::Idle).status_line(),
            "Connected as Ada"
        );
        assert_eq!(
            card(Some(session("")), Phase::Idle).status_line(),
            "Connected"
        );
        assert_eq!(
            card(None, Phase::WaitingForBrowser).status_line(),
            "Waiting for the browser…"
        );
    }

    #[test]
    fn signing_in_needs_a_client_and_no_sign_in_underway() {
        assert!(card(None, Phase::Idle).can_sign_in());
        assert!(!card(None, Phase::WaitingForBrowser).can_sign_in());
        let mut without_client = card(None, Phase::Idle);
        without_client.account.client.id = "  ".to_owned();
        assert!(!without_client.can_sign_in());
    }
}
